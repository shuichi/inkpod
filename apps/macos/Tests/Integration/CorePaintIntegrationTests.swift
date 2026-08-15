import XCTest
@testable import InkpodCoreBridge

final class CorePaintIntegrationTests: XCTestCase {
    func testM7SelectionAlgebraHistoryBranchAndBoundedVisualizationContracts() throws {
        let fixture = try makeColorPlaneFixture(low: 7)
        defer { shutdown(fixture.host) }

        var paint = try requirePaint(
            fixture.host.inspectPaint(target: fixture.session.target)
        )
        XCTAssertEqual(paint.editor.selectionOptions.aspectRatio, 0)
        XCTAssertEqual(
            outcome(fixture.host.updateEditor(
                target: fixture.session.primaryView,
                expectation: paint.editor.expectation,
                update: .selectionOptions(paint.editor.selectionOptions)
            )),
            .noOp(paint.editor.session)
        )
        _ = try requireDocument(fixture.host.selectColor(
            target: fixture.session.primaryView,
            expectation: paint.editor.expectation,
            different: true,
            operation: .replace
        ))
        paint = try requirePaint(
            fixture.host.inspectPaint(target: fixture.session.target)
        )
        paint = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: paint.editor.expectation,
            update: .selectionOptions(CoreSelectionOptions(
                shape: .rectangle,
                operation: .replace
            ))
        ))
        paint = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: paint.editor.expectation,
            update: .activeTool(.selection)
        ))

        let first = try requireDocument(fixture.host.applySelection(
            target: fixture.session.primaryView,
            expectation: paint.editor.expectation,
            samples: [
                .init(deviceX: 1, deviceY: 2, pressure: 1),
                .init(deviceX: 11, deviceY: 12, pressure: 1),
            ]
        ))
        XCTAssertEqual(first.documentRevision, paint.editor.session.documentRevision + 1)

        let afterFirst = try requirePaint(
            fixture.host.inspectPaint(target: fixture.session.target)
        )
        XCTAssertEqual(
            outcome(fixture.host.applySelection(
                target: fixture.session.primaryView,
                expectation: afterFirst.editor.expectation,
                samples: [
                    .init(deviceX: 1, deviceY: 2, pressure: 1),
                    .init(deviceX: 11, deviceY: 12, pressure: 1),
                ]
            )),
            .noOp(first)
        )
        XCTAssertEqual(
            outcome(fixture.host.applySelection(
                target: fixture.session.primaryView,
                expectation: afterFirst.editor.expectation,
                samples: [.init(deviceX: .nan, deviceY: 1, pressure: 1)]
            )),
            .failed(.invalidRequest)
        )
        XCTAssertEqual(
            outcome(fixture.host.selectionAdjust(
                target: fixture.session.target,
                expectedDocumentRevision: first.documentRevision,
                operation: .expand,
                pixels: 4_097
            )),
            .failed(.invalidRequest)
        )

        let layered = try requireDocument(fixture.host.selectionToLayer(
            target: fixture.session.target,
            expectedDocumentRevision: first.documentRevision,
            nameUTF8: Array("Selection Contract".utf8)
        ))
        let tree = try requireTree(fixture.host.inspectTree(target: fixture.session.target))
        let selectionLayer = try XCTUnwrap(tree.layers.first { $0.kind == .selection })
        let selectionCleared = try requireDocument(fixture.host.clearSelection(
            target: fixture.session.target,
            expectedDocumentRevision: layered.documentRevision
        ))
        let restored = try requireDocument(fixture.host.selectionFromLayer(
            target: fixture.session.target,
            expectedDocumentRevision: selectionCleared.documentRevision,
            layerID: selectionLayer.id,
            operation: .replace
        ))
        XCTAssertEqual(
            outcome(fixture.host.selectionFromLayer(
                target: fixture.session.target,
                expectedDocumentRevision: restored.documentRevision,
                layerID: selectionLayer.id,
                operation: .add
            )),
            .noOp(restored)
        )
        let subtracted = try requireDocument(fixture.host.selectionFromLayer(
            target: fixture.session.target,
            expectedDocumentRevision: restored.documentRevision,
            layerID: selectionLayer.id,
            operation: .subtract
        ))
        let selectedAgain = try requireDocument(fixture.host.selectionFromLayer(
            target: fixture.session.target,
            expectedDocumentRevision: subtracted.documentRevision,
            layerID: selectionLayer.id,
            operation: .replace
        ))
        XCTAssertEqual(
            outcome(fixture.host.selectionFromLayer(
                target: fixture.session.target,
                expectedDocumentRevision: selectedAgain.documentRevision,
                layerID: 0,
                operation: .replace
            )),
            .failed(.invalidRequest)
        )
        XCTAssertEqual(
            outcome(fixture.host.selectionAdjust(
                target: fixture.session.target,
                expectedDocumentRevision: selectedAgain.documentRevision + 1,
                operation: .invert,
                pixels: 0
            )),
            .failed(.staleTarget)
        )

        let inverted = try requireDocument(fixture.host.selectionAdjust(
            target: fixture.session.target,
            expectedDocumentRevision: selectedAgain.documentRevision,
            operation: .invert,
            pixels: 0
        ))
        let expanded = try requireDocument(fixture.host.selectionAdjust(
            target: fixture.session.target,
            expectedDocumentRevision: inverted.documentRevision,
            operation: .expand,
            pixels: 1
        ))
        let shrunk = try requireDocument(fixture.host.selectionAdjust(
            target: fixture.session.target,
            expectedDocumentRevision: expanded.documentRevision,
            operation: .shrink,
            pixels: 1
        ))

        let history = try requireHistory(fixture.host.inspectHistory(
            target: fixture.session.target,
            expectedDocumentRevision: shrunk.documentRevision
        ))
        XCTAssertEqual(history.cursor, UInt64(history.items.count))
        XCTAssertTrue(history.items.allSatisfy(\.isApplied))

        let undone = try requireDocument(fixture.host.undo(
            target: fixture.session.target,
            expectedDocumentRevision: shrunk.documentRevision
        ))
        let redone = try requireDocument(fixture.host.redo(
            target: fixture.session.target,
            expectedDocumentRevision: undone.documentRevision
        ))
        XCTAssertEqual(redone.canUndo, shrunk.canUndo)
        XCTAssertFalse(redone.canRedo)

        let branchedBase = try requireDocument(fixture.host.undo(
            target: fixture.session.target,
            expectedDocumentRevision: redone.documentRevision
        ))
        let cleared = try requireDocument(fixture.host.clearSelection(
            target: fixture.session.target,
            expectedDocumentRevision: branchedBase.documentRevision
        ))
        XCTAssertFalse(cleared.canRedo)
        XCTAssertEqual(
            outcome(fixture.host.beginHistoryVisualization(
                target: fixture.session.target,
                expectedDocumentRevision: cleared.documentRevision + 1
            )),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            outcome(fixture.host.jumpHistory(
                target: fixture.session.target,
                expectedDocumentRevision: cleared.documentRevision + 1,
                cursor: 0
            )),
            .failed(.staleTarget)
        )

        var progress = try requireHistoryVisualizationProgress(
            fixture.host.beginHistoryVisualization(
                target: fixture.session.target,
                expectedDocumentRevision: cleared.documentRevision
            )
        )
        while !progress.isComplete {
            progress = try requireHistoryVisualizationProgress(
                fixture.host.stepHistoryVisualization(progress.id, maximumEvents: 1)
            )
        }
        let rows = try requireHistoryVisualizationRows(
            fixture.host.historyVisualizationRows(
                progress.id,
                range: 0 ..< progress.rowCount
            )
        )
        XCTAssertEqual(rows.count, Int(progress.rowCount))
        XCTAssertFalse(rows.isEmpty)
        XCTAssertGreaterThanOrEqual(Set(rows.map(\.branchID)).count, 2)
        XCTAssertEqual(
            outcome(fixture.host.releaseHistoryVisualization(progress.id)),
            .acknowledged
        )
        XCTAssertEqual(
            outcome(fixture.host.releaseHistoryVisualization(progress.id)),
            .noOp(nil)
        )
        XCTAssertEqual(
            outcome(fixture.host.historyVisualizationRows(progress.id, range: 0 ..< 0)),
            .failed(.staleTarget)
        )
        let cancelled = try requireHistoryVisualizationProgress(
            fixture.host.beginHistoryVisualization(
                target: fixture.session.target,
                expectedDocumentRevision: cleared.documentRevision
            )
        )
        XCTAssertEqual(
            outcome(fixture.host.releaseHistoryVisualization(cancelled.id)),
            .acknowledged
        )
        XCTAssertEqual(
            outcome(fixture.host.releaseHistoryVisualization(cancelled.id)),
            .noOp(nil)
        )
        let closeOwned = try requireHistoryVisualizationProgress(
            fixture.host.beginHistoryVisualization(
                target: fixture.session.target,
                expectedDocumentRevision: cleared.documentRevision
            )
        )
        guard case .closed = outcome(fixture.host.closeSession(fixture.session.target)) else {
            XCTFail("session close did not release the live history builder")
            return
        }
        XCTAssertEqual(
            outcome(fixture.host.releaseHistoryVisualization(closeOwned.id)),
            .noOp(nil)
        )
    }

    func testEditorUpdatesPreserveExactDepthAndRejectInvalidOrStaleExpectations() throws {
        let fixture = try makeColorPlaneFixture(low: 1)
        defer { shutdown(fixture.host) }

        let initial = try requirePaint(
            fixture.host.inspectPaint(
                target: fixture.session.target,
                expectedDocumentRevision: fixture.session.documentRevision
            )
        )
        XCTAssertEqual(initial.editor.activePlaneID, fixture.colorPlaneID)

        let exact16 = CoreColorValue.rgba16(
            red: 0x1234,
            green: 0x5678,
            blue: 0x9ABC,
            alpha: 0xDEF0
        )
        let changed = try requirePaintUpdated(
            fixture.host.updateEditor(
                target: fixture.session.primaryView,
                expectation: initial.editor.expectation,
                update: .toolColor(exact16)
            )
        )
        XCTAssertEqual(changed.editor.currentColor, exact16)
        XCTAssertEqual(
            changed.editor.session.documentRevision,
            initial.editor.session.documentRevision
        )
        XCTAssertGreaterThan(changed.editor.editorRevision, initial.editor.editorRevision)

        XCTAssertEqual(
            outcome(fixture.host.updateEditor(
                target: fixture.session.primaryView,
                expectation: initial.editor.expectation,
                update: .activeTool(.brush)
            )),
            .failed(.staleTarget)
        )
        XCTAssertEqual(
            outcome(fixture.host.updateEditor(
                target: fixture.session.primaryView,
                expectation: changed.editor.expectation,
                update: .diameter(.nan)
            )),
            .failed(.invalidRequest)
        )
        XCTAssertEqual(
            outcome(fixture.host.updateEditor(
                target: fixture.session.primaryView,
                expectation: changed.editor.expectation,
                update: .toolColor(CoreColorValue(
                    depth: .rgba8,
                    red: 256,
                    green: 0,
                    blue: 0,
                    alpha: 255
                ))
            )),
            .failed(.invalidRequest)
        )
        XCTAssertEqual(
            outcome(fixture.host.updateEditor(
                target: fixture.session.primaryView,
                expectation: changed.editor.expectation,
                update: .fillOptions(CoreFillOptions(gapClose: 256))
            )),
            .failed(.invalidRequest)
        )

        let unchanged = try requirePaint(
            fixture.host.inspectPaint(target: fixture.session.target)
        )
        XCTAssertEqual(unchanged.editor.currentColor, exact16)
        XCTAssertEqual(unchanged.editor.editorRevision, changed.editor.editorRevision)
    }

    func testBrushCancelCommitFillEyedropperAndLocatorUseFixedGestureTarget() throws {
        let fixture = try makeColorPlaneFixture(low: 2)
        defer { shutdown(fixture.host) }
        _ = try requireView(
            fixture.host.applyView(
                target: fixture.session.primaryView,
                command: .viewportResized(width: 1_920, height: 1_080)
            )
        )
        var paint = try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
        paint = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: paint.editor.expectation,
            update: .activeTool(.brush)
        ))
        paint = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: paint.editor.expectation,
            update: .toolColor(.rgba8(red: 220, green: 30, blue: 40))
        ))

        XCTAssertEqual(
            outcome(fixture.host.beginRasterStroke(
                target: fixture.session.primaryView,
                expectation: paint.editor.expectation,
                samples: [.init(deviceX: 20, deviceY: 20, pressure: 0.5)]
            )),
            .acknowledged
        )
        XCTAssertEqual(outcome(fixture.host.cancelStroke(target: fixture.session.primaryView)), .acknowledged)
        let cancelled = try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
        XCTAssertEqual(
            cancelled.editor.session.documentRevision,
            paint.editor.session.documentRevision
        )

        XCTAssertEqual(
            outcome(fixture.host.beginRasterStroke(
                target: fixture.session.primaryView,
                expectation: cancelled.editor.expectation,
                samples: [.init(deviceX: 20, deviceY: 20, pressure: 0.5)]
            )),
            .acknowledged
        )
        XCTAssertEqual(
            outcome(fixture.host.appendRasterStroke(
                target: fixture.session.primaryView,
                samples: [.init(deviceX: 24, deviceY: 24, pressure: 1)]
            )),
            .acknowledged
        )
        let committed = try requireDocument(fixture.host.endStroke(target: fixture.session.primaryView))
        XCTAssertEqual(committed.documentRevision, cancelled.editor.session.documentRevision + 1)

        var afterStroke = try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
        afterStroke = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: afterStroke.editor.expectation,
            update: .activeTool(.eraser)
        ))
        XCTAssertEqual(
            outcome(fixture.host.beginRasterStroke(
                target: fixture.session.primaryView,
                expectation: afterStroke.editor.expectation,
                samples: [.init(deviceX: 500, deviceY: 500, pressure: 1)]
            )),
            .acknowledged
        )
        XCTAssertEqual(
            outcome(fixture.host.endStroke(target: fixture.session.primaryView)),
            .noOp(afterStroke.editor.session)
        )
        afterStroke = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: afterStroke.editor.expectation,
            update: .activeTool(.brush)
        ))
        let sampled = try requireEyedropper(fixture.host.eyedropper(
            target: fixture.session.primaryView,
            expectation: afterStroke.editor.expectation,
            source: .selectedPlane,
            devicePoint: .init(deviceX: 20, deviceY: 20, pressure: 1)
        ))
        XCTAssertEqual(sampled.editor.currentColor, .rgba8(red: 220, green: 30, blue: 40))

        let locator = try requireLocator(fixture.host.inspectLocator(
            target: fixture.session.primaryView,
            expectedViewRevision: sampled.editor.session.viewRevision,
            devicePoint: .init(deviceX: 20, deviceY: 20, pressure: 1),
            radius: 2
        ))
        XCTAssertEqual(locator.documentX, 20)
        XCTAssertEqual(locator.documentY, 20)
        XCTAssertEqual(locator.neighborhoodRGBA8.count, 5 * 5 * 4)

        let locatorPainted = try requireDocument(fixture.host.paintLocatorPixel(
            target: fixture.session.primaryView,
            expectation: sampled.editor.expectation,
            documentX: 20,
            documentY: 20
        ))
        XCTAssertEqual(locatorPainted.documentRevision, committed.documentRevision + 1)

        let afterLocator = try requirePaint(
            fixture.host.inspectPaint(target: fixture.session.target)
        )
        let fillReady = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: afterLocator.editor.expectation,
            update: .fillOptions(CoreFillOptions(
                operation: .seed,
                useDocumentSelection: true,
                tolerance: 257,
                gapClose: 2
            ))
        ))
        XCTAssertEqual(fillReady.editor.fillOptions.gapClose, 2)
        XCTAssertEqual(fillReady.editor.fillOptions.tolerance, 257)
        XCTAssertTrue(fillReady.editor.fillOptions.useDocumentSelection)
        XCTAssertEqual(
            outcome(fixture.host.applyFill(
                target: fixture.session.primaryView,
                expectation: fillReady.editor.expectation,
                gesture: .init(
                    start: .init(deviceX: 100, deviceY: 100, pressure: 1),
                    end: .init(deviceX: 100, deviceY: 100, pressure: 1)
                )
            )),
            .noOp(fillReady.editor.session)
        )
        let fillWithoutSelection = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: fillReady.editor.expectation,
            update: .fillOptions(CoreFillOptions(
                operation: .seed,
                useDocumentSelection: false,
                tolerance: 257,
                gapClose: 2
            ))
        ))
        XCTAssertEqual(
            outcome(fixture.host.applyFill(
                target: fixture.session.primaryView,
                expectation: fillWithoutSelection.editor.expectation,
                gesture: .init(
                    start: .init(deviceX: 100, deviceY: 100, pressure: 1),
                    end: .init(deviceX: 100, deviceY: 100, pressure: 1)
                )
            )),
            .failed(.coreOperation(.fillOverflow))
        )
        let committingFill = try requirePaintUpdated(fixture.host.updateEditor(
            target: fixture.session.primaryView,
            expectation: fillWithoutSelection.editor.expectation,
            update: .fillOptions(CoreFillOptions(
                operation: .seed,
                overflowAbort: false,
                useDocumentSelection: false,
                tolerance: 257,
                gapClose: 2
            ))
        ))
        let filled = try requireFill(fixture.host.applyFill(
            target: fixture.session.primaryView,
            expectation: committingFill.editor.expectation,
            gesture: .init(
                start: .init(deviceX: 100, deviceY: 100, pressure: 1),
                end: .init(deviceX: 100, deviceY: 100, pressure: 1)
            )
        ))
        XCTAssertGreaterThan(filled.changedPixelCount, 0)
        XCTAssertEqual(filled.session.documentRevision, locatorPainted.documentRevision + 1)
        let guarded = try requireOutputGuard(fixture.host.selectOutputColorGuard(
            target: fixture.session.target,
            expectedDocumentRevision: filled.session.documentRevision,
            operation: .replace
        ))
        XCTAssertGreaterThan(guarded.scannedPixelCount, 0)
        XCTAssertGreaterThan(guarded.selectedPixelCount, 0)
    }

    func testPaletteChartPreviewCancelApplyAndColorReplacementAreAtomic() throws {
        let fixture = try makeColorPlaneFixture(low: 3)
        defer { shutdown(fixture.host) }
        let initial = try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
        let colors = [
            CoreColorValue.rgba8(red: 10, green: 20, blue: 30),
            CoreColorValue.rgba16(red: 1_000, green: 2_000, blue: 3_000, alpha: 4_000),
        ]
        let palette = try requirePaintUpdated(fixture.host.replacePalette(
            target: fixture.session.target,
            expectedDocumentRevision: initial.editor.session.documentRevision,
            colors: colors
        ))
        XCTAssertEqual(palette.palette.colors, colors)

        let preview = try requireChartPreview(fixture.host.createColorChartPreview(
            target: fixture.session.target,
            expectedDocumentRevision: palette.editor.session.documentRevision,
            maximumColors: 16,
            quantizationBits: 5
        ))
        XCTAssertEqual(preview.baseDocumentRevision, palette.editor.session.documentRevision)
        XCTAssertEqual(outcome(fixture.host.cancelColorChartPreview(preview.id)), .acknowledged)
        XCTAssertEqual(
            try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
                .editor.session.documentRevision,
            palette.editor.session.documentRevision
        )

        let secondPreview = try requireChartPreview(fixture.host.createColorChartPreview(
            target: fixture.session.target,
            expectedDocumentRevision: palette.editor.session.documentRevision,
            maximumColors: 16,
            quantizationBits: 5
        ))
        let applied = try requirePaintUpdated(fixture.host.applyColorChartPreview(
            target: fixture.session.target,
            expectedDocumentRevision: palette.editor.session.documentRevision,
            preview: secondPreview.id
        ))
        XCTAssertEqual(applied.editor.session.documentRevision, palette.editor.session.documentRevision + 1)
        XCTAssertEqual(
            outcome(fixture.host.applyColorChartPreview(
                target: fixture.session.target,
                expectedDocumentRevision: applied.editor.session.documentRevision,
                preview: secondPreview.id
            )),
            .failed(.staleTarget)
        )

        let request = CoreColorReplaceRequest(
            mode: .rasterColor,
            targetColor: .rgba8(red: 0, green: 0, blue: 0, alpha: 0),
            replacementColor: .rgba8(red: 1, green: 2, blue: 3),
            region: .rectangle(.init(
                start: .init(deviceX: 1, deviceY: 1, pressure: 1),
                end: .init(deviceX: 4, deviceY: 4, pressure: 1)
            ))
        )
        let beforeReplace = try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
        _ = try requireReplacePreview(fixture.host.previewColorReplace(
            target: fixture.session.primaryView,
            expectation: beforeReplace.editor.expectation,
            request: request
        ))
        let regions: [CoreColorReplaceRegion] = [
            .pen(samples: [
                .init(deviceX: 1, deviceY: 1, pressure: 0.5),
                .init(deviceX: 4, deviceY: 4, pressure: 1),
            ], diameter: 4),
            .polyline([
                .init(deviceX: 1, deviceY: 1, pressure: 1),
                .init(deviceX: 4, deviceY: 1, pressure: 1),
                .init(deviceX: 4, deviceY: 4, pressure: 1),
            ]),
            .lasso([
                .init(deviceX: 1, deviceY: 1, pressure: 1),
                .init(deviceX: 4, deviceY: 1, pressure: 1),
                .init(deviceX: 4, deviceY: 4, pressure: 1),
                .init(deviceX: 1, deviceY: 1, pressure: 1),
            ]),
        ]
        for region in regions {
            _ = try requireReplacePreview(fixture.host.previewColorReplace(
                target: fixture.session.primaryView,
                expectation: beforeReplace.editor.expectation,
                request: CoreColorReplaceRequest(
                    mode: .rasterColor,
                    targetColor: request.targetColor,
                    replacementColor: request.replacementColor,
                    region: region
                )
            ))
        }
        XCTAssertEqual(
            try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
                .editor.session.documentRevision,
            beforeReplace.editor.session.documentRevision
        )
    }

    func testOutputGuardInvalidStaleAndCancellationPreserveDocumentState() throws {
        let fixture = try makeColorPlaneFixture(low: 4)
        defer { shutdown(fixture.host) }
        let before = try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))

        XCTAssertEqual(
            outcome(fixture.host.selectOutputColorGuard(
                target: fixture.session.target,
                expectedDocumentRevision: before.editor.session.documentRevision + 1,
                operation: .replace
            )),
            .failed(.staleTarget)
        )
        let task = fixture.host.selectOutputColorGuard(
            target: fixture.session.target,
            expectedDocumentRevision: before.editor.session.documentRevision,
            operation: .replace
        )
        _ = fixture.host.cancel(request: task.requestID)
        XCTAssertEqual(outcome(task), .failed(.cancelled))
        XCTAssertEqual(
            try requirePaint(fixture.host.inspectPaint(target: fixture.session.target))
                .editor.session.documentRevision,
            before.editor.session.documentRevision
        )
    }

    private struct Fixture {
        let host: CoreHost
        let session: CoreSessionProjection
        let colorPlaneID: UInt64
    }

    private func makeColorPlaneFixture(low: UInt64) throws -> Fixture {
        let host = CoreHost()
        let session = try requireCreated(host.createSession(
            documentUUID: .init(high: 0xA600, low: low)
        ))
        let tree = try requireTree(host.inspectTree(target: session.target))
        let colorPlane = try XCTUnwrap(
            tree.layers.flatMap(\.planes).first { $0.kind == .color || $0.kind == .raster }
        )
        let parent = try XCTUnwrap(tree.layers.first { layer in
            layer.planes.contains { $0.id == colorPlane.id }
        })
        _ = outcome(host.setActiveNode(
            target: session.target,
            layerID: parent.id,
            planeID: colorPlane.id,
            expectedDocumentRevision: session.documentRevision
        ))
        return Fixture(host: host, session: session, colorPlaneID: colorPlane.id)
    }

    private func outcome(_ task: CoreTask) -> CoreRequestOutcome {
        task.wait(timeout: 30) ?? .failed(.cancelled)
    }

    private func requireCreated(_ task: CoreTask) throws -> CoreSessionProjection {
        guard case let .created(value) = outcome(task) else { throw Failure.unexpected }
        return value
    }

    private func requireView(_ task: CoreTask) throws -> CoreSessionProjection {
        guard case let .viewUpdated(value) = outcome(task) else { throw Failure.unexpected }
        return value
    }

    private func requireDocument(_ task: CoreTask) throws -> CoreSessionProjection {
        guard case let .documentUpdated(value) = outcome(task) else { throw Failure.unexpected }
        return value
    }

    private func requireTree(_ task: CoreTask) throws -> CoreTreeProjection {
        guard case let .tree(value) = outcome(task) else { throw Failure.unexpected }
        return value
    }

    private func requirePaint(_ task: CoreTask) throws -> CorePaintProjection {
        let result = outcome(task)
        guard case let .paint(value) = result else {
            XCTFail("expected paint projection, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requirePaintUpdated(_ task: CoreTask) throws -> CorePaintProjection {
        let result = outcome(task)
        guard case let .paintUpdated(value) = result else {
            XCTFail("expected paint update, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireFill(_ task: CoreTask) throws -> CoreFillProjection {
        let result = outcome(task)
        guard case let .fillApplied(value) = result else {
            XCTFail("expected fill, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireEyedropper(_ task: CoreTask) throws -> CorePaintProjection {
        let result = outcome(task)
        guard case let .eyedropperSampled(value) = result else {
            XCTFail("expected eyedropper, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireOutputGuard(_ task: CoreTask) throws -> CoreOutputColorGuardProjection {
        let result = outcome(task)
        guard case let .outputColorGuardApplied(value) = result else {
            XCTFail("expected output guard, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireLocator(_ task: CoreTask) throws -> CoreLocatorProjection {
        let result = outcome(task)
        guard case let .locator(value) = result else {
            XCTFail("expected locator, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireChartPreview(_ task: CoreTask) throws -> CoreColorChartPreviewProjection {
        let result = outcome(task)
        guard case let .colorChartPreview(value) = result else {
            XCTFail("expected chart preview, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireReplacePreview(_ task: CoreTask) throws -> CoreColorReplacePreviewProjection {
        let result = outcome(task)
        guard case let .colorReplacePreview(value) = result else {
            XCTFail("expected replacement preview, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireHistory(_ task: CoreTask) throws -> CoreHistoryProjection {
        let result = outcome(task)
        guard case let .history(value) = result else {
            XCTFail("expected history, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireHistoryVisualizationProgress(
        _ task: CoreTask
    ) throws -> CoreHistoryVisualizationProgressProjection {
        let result = outcome(task)
        guard case let .historyVisualizationProgress(value) = result else {
            XCTFail("expected history visualization progress, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func requireHistoryVisualizationRows(
        _ task: CoreTask
    ) throws -> [CoreHistoryVisualizationRow] {
        let result = outcome(task)
        guard case let .historyVisualizationRows(value) = result else {
            XCTFail("expected history visualization rows, received \(result)")
            throw Failure.unexpected
        }
        return value
    }

    private func shutdown(_ host: CoreHost) {
        _ = outcome(host.shutdown())
        XCTAssertTrue(host.waitUntilStopped(timeout: 30))
    }
}

private enum Failure: Error {
    case unexpected
}
