import AppKit
import QuartzCore
import SwiftUI
import XCTest
@testable import InkpodCoreBridge

final class ProductCanvasLifecycleTests: XCTestCase {
    @MainActor
    func testM11ChromeCommandsPreserveDocumentHistoryAndDirtyState() async throws {
        _ = NSApplication.shared
        let application = ApplicationCoordinator()
        let workspaceID = WorkspaceID(
            rawValue: UUID(uuidString: "A1110000-0000-0000-0000-000000000011")!
        )
        let workspace = application.workspace(for: workspaceID)
        workspace.start()
        let hostingView = NSHostingView(
            rootView: InkpodWorkspaceScene(id: workspaceID, application: application)
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: 1_200, height: 800)
        let window = NSWindow(
            contentRect: hostingView.frame,
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentView = hostingView
        window.makeKeyAndOrderFront(nil)
        defer {
            dismantleProductCanvas(in: window)
        }

        guard await waitUntil(timeout: 30, condition: {
            workspace.phase == .ready
                && workspace.commandContext != nil
                && workspace.history != nil
        }), let before = workspace.projection, let context = workspace.commandContext
        else {
            XCTFail("M11 product workspace did not become ready")
            await application.shutdown(confirmingDirty: false)
            return
        }
        let beforeHistory = workspace.history
        let foundCanvas = await waitForCanvas(in: hostingView, timeout: 10)
        let canvas = try XCTUnwrap(foundCanvas)
        for command in [
            InkpodCommandID.windowToolPalette,
            .windowToolOptions,
            .windowLayerPalette,
            .windowColorPane,
            .workspaceMirror,
        ] {
            XCTAssertEqual(workspace.execute(command, context: context), .started)
        }
        await Task.yield()
        let after = try XCTUnwrap(workspace.projection)
        XCTAssertEqual(after.target, before.target)
        XCTAssertEqual(after.documentRevision, before.documentRevision)
        XCTAssertEqual(after.canUndo, before.canUndo)
        XCTAssertEqual(after.canRedo, before.canRedo)
        XCTAssertEqual(after.isDirty, before.isDirty)
        XCTAssertEqual(workspace.history?.cursor, beforeHistory?.cursor)
        XCTAssertEqual(workspace.history?.items, beforeHistory?.items)

        let explicitChrome = workspace.chromePreference
        for width in [640.0, 800.0, 1_200.0] {
            window.setContentSize(NSSize(width: width, height: 800))
            hostingView.layoutSubtreeIfNeeded()
            let drawableMatched = await waitUntil(timeout: 5) {
                guard let layer = canvas.layer as? CAMetalLayer else { return false }
                return layer.drawableSize == canvas.convertToBacking(canvas.bounds).size
            }
            XCTAssertTrue(drawableMatched)
            let backingBounds = canvas.convertToBacking(canvas.bounds)
            XCTAssertGreaterThan(backingBounds.width, 0)
            XCTAssertGreaterThan(backingBounds.height, 0)
            XCTAssertNotNil(CanvasInputNormalizer.sample(
                deviceX: backingBounds.width.nextDown,
                deviceY: backingBounds.height.nextDown,
                drawableWidth: backingBounds.width,
                drawableHeight: backingBounds.height,
                pressure: nil,
                tilt: nil
            ))
            XCTAssertNil(CanvasInputNormalizer.sample(
                deviceX: backingBounds.width,
                deviceY: backingBounds.height,
                drawableWidth: backingBounds.width,
                drawableHeight: backingBounds.height,
                pressure: nil,
                tilt: nil
            ))
        }
        XCTAssertEqual(workspace.chromePreference, explicitChrome)
        let chromeRestored = await waitUntil(timeout: 5) {
            workspace.adaptiveChrome.toolPresentation == .compact
                && workspace.adaptiveChrome.inspectorVisible
        }
        XCTAssertTrue(chromeRestored)
        XCTAssertEqual(workspace.adaptiveChrome.toolPresentation, .compact)
        XCTAssertTrue(workspace.adaptiveChrome.inspectorVisible)

        let automaticSuspensionContext = try XCTUnwrap(workspace.commandContext)
        let automaticSuspensionView = automaticSuspensionContext.view
        window.setContentSize(NSSize(width: 640, height: 800))
        hostingView.layoutSubtreeIfNeeded()
        workspace.updateAdaptiveChrome(availableWidth: 640)
        XCTAssertTrue(workspace.chromePreference.inspectorRequestedVisible)
        XCTAssertFalse(workspace.adaptiveChrome.inspectorVisible)
        XCTAssertEqual(
            workspace.execute(.viewNew, context: automaticSuspensionContext),
            .started
        )
        let automaticReplacementActivated = await waitUntil(timeout: 5) {
            workspace.commandContext?.view != automaticSuspensionView
        }
        XCTAssertTrue(automaticReplacementActivated)
        window.setContentSize(NSSize(width: 1_200, height: 800))
        hostingView.layoutSubtreeIfNeeded()
        workspace.updateAdaptiveChrome(availableWidth: 1_200)
        XCTAssertEqual(workspace.lastCommandResult, .stale)
        XCTAssertTrue(workspace.chromePreference.inspectorRequestedVisible)
        XCTAssertFalse(workspace.adaptiveChrome.inspectorVisible)
        let automaticReplacementContext = try XCTUnwrap(workspace.commandContext)
        XCTAssertEqual(
            workspace.execute(.windowColorPane, context: automaticReplacementContext),
            .started
        )
        XCTAssertFalse(workspace.chromePreference.inspectorRequestedVisible)
        XCTAssertFalse(workspace.adaptiveChrome.inspectorVisible)
        XCTAssertEqual(
            workspace.execute(.windowColorPane, context: automaticReplacementContext),
            .started
        )
        XCTAssertTrue(workspace.adaptiveChrome.inspectorVisible)

        let colorContext = try XCTUnwrap(workspace.commandContext)
        XCTAssertEqual(workspace.execute(.windowColorPane, context: colorContext), .started)
        XCTAssertFalse(workspace.adaptiveChrome.inspectorVisible)
        let suspendedView = colorContext.view
        XCTAssertEqual(workspace.execute(.viewNew, context: colorContext), .started)
        let replacementActivated = await waitUntil(timeout: 5) {
            workspace.commandContext?.view != suspendedView
        }
        XCTAssertTrue(replacementActivated)
        let replacementContext = try XCTUnwrap(workspace.commandContext)
        let closedChrome = workspace.chromePreference
        XCTAssertEqual(
            workspace.execute(.windowColorPane, context: replacementContext),
            .stale
        )
        XCTAssertEqual(workspace.lastCommandResult, .stale)
        XCTAssertEqual(workspace.chromePreference, closedChrome)
        XCTAssertFalse(workspace.adaptiveChrome.inspectorVisible)
        workspace.locatorVisible = false
        XCTAssertEqual(workspace.execute(.windowLocator, context: replacementContext), .stale)
        XCTAssertFalse(workspace.locatorVisible)
        workspace.lightTableVisible = false
        XCTAssertEqual(workspace.execute(.windowLightTable, context: replacementContext), .stale)
        XCTAssertFalse(workspace.lightTableVisible)

        await application.shutdown(confirmingDirty: false)
    }

    @MainActor
    func testM11TwoHundredChromeResizesReuseTilesAndReleaseSnapshotsExactlyOnce() async throws {
        _ = NSApplication.shared
        let host = CoreHost()
        let renderer = MetalRendererHost()
        let surface = CoreSurfaceTarget(
            id: .init(rawValue: 1_111),
            generation: .init(rawValue: 1)
        )
        let metalLayer = CAMetalLayer()
        metalLayer.frame = CGRect(x: 0, y: 0, width: 128, height: 128)
        metalLayer.drawableSize = CGSize(width: 128, height: 128)
        let container = NSView(frame: metalLayer.frame)
        container.wantsLayer = true
        container.layer = metalLayer
        let window = NSWindow(
            contentRect: container.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.contentView = container
        window.makeKeyAndOrderFront(nil)
        defer {
            window.orderOut(nil)
            window.contentView = nil
            _ = renderer.shutdown()
            _ = host.shutdown().wait(timeout: 20)
        }

        let createdOutcome = await host.createSession(
            documentUUID: .init(high: 0xA111, low: 2)
        ).value()
        guard case let .created(created) = createdOutcome else {
            XCTFail("M11 Core session creation failed: \(createdOutcome)")
            return
        }
        guard case let .viewUpdated(viewport) = await host.applyView(
            target: created.primaryView,
            command: .viewportResized(width: 128, height: 128)
        ).value(),
            case let .paint(paint) = await host.inspectPaint(
                target: created.target,
                expectedDocumentRevision: viewport.documentRevision
            ).value(),
            case let .paintUpdated(brush) = await host.updateEditor(
                target: viewport.primaryView,
                expectation: paint.editor.expectation,
                update: .activeTool(.brush)
            ).value(),
            case .acknowledged = await host.beginRasterStroke(
                target: viewport.primaryView,
                expectation: brush.editor.expectation,
                samples: [.init(deviceX: 32, deviceY: 32, pressure: 1)]
            ).value(),
            case .acknowledged = await host.appendRasterStroke(
                target: viewport.primaryView,
                samples: [.init(deviceX: 96, deviceY: 96, pressure: 1)]
            ).value(),
            case .documentUpdated = await host.endStroke(
                target: viewport.primaryView
            ).value()
        else {
            XCTFail("M11 tile-reuse fixture could not commit its seed stroke")
            return
        }
        let route = CoreSnapshotRoute(
            session: created.target,
            view: viewport.primaryView,
            surface: surface
        )
        XCTAssertTrue(renderer.registerSurface(
            route: route,
            layer: metalLayer,
            drawableSize: metalLayer.drawableSize
        ))
        var snapshots: [CoreSnapshotEnvelope] = []
        snapshots.reserveCapacity(200)
        for index in 0 ..< 200 {
            let width = index.isMultiple(of: 2) ? 128.0 : 96.0
            renderer.resizeSurface(surface, drawableSize: CGSize(width: width, height: 128))
            let outcome = await host.buildSnapshot(route: route).value()
            guard case let .snapshot(snapshot) = outcome else {
                XCTFail("M11 snapshot build failed at \(index): \(outcome)")
                break
            }
            snapshots.append(snapshot)
            let submission = renderer.submit(snapshot)
            XCTAssertTrue(submission == .accepted || submission == .replacedPending)
            if index.isMultiple(of: 20) {
                XCTAssertTrue(renderer.waitUntilIdle(timeout: 10))
            }
        }
        XCTAssertEqual(snapshots.count, 200)
        XCTAssertTrue(renderer.waitUntilIdle(timeout: 20))
        let metrics = renderer.metrics()
        XCTAssertEqual(metrics.hiddenDrawCount, 0)
        XCTAssertGreaterThan(metrics.reusedTileCount, 0)
        XCTAssertLessThanOrEqual(metrics.uploadedTileCount, 4)
        XCTAssertGreaterThan(metrics.reusedTileCount, metrics.uploadedTileCount)
        XCTAssertTrue(renderer.unregisterSurface(surface))
        XCTAssertTrue(snapshots.allSatisfy { $0.owner.ffiReleaseCount == 1 })
    }

    @MainActor
    func testM9LightTableSnapshotPresentsThroughMetalAndReleasesExactlyOnce() async throws {
        _ = NSApplication.shared
        let host = CoreHost()
        let renderer = MetalRendererHost()
        let surface = CoreSurfaceTarget(
            id: .init(rawValue: 909),
            generation: .init(rawValue: 1)
        )
        let metalLayer = CAMetalLayer()
        metalLayer.frame = CGRect(x: 0, y: 0, width: 128, height: 128)
        metalLayer.drawableSize = CGSize(width: 128, height: 128)
        let container = NSView(frame: metalLayer.frame)
        container.wantsLayer = true
        container.layer = metalLayer
        let window = NSWindow(
            contentRect: container.frame,
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.contentView = container
        window.makeKeyAndOrderFront(nil)
        defer {
            window.orderOut(nil)
            window.contentView = nil
            _ = renderer.unregisterSurface(surface)
            _ = renderer.shutdown()
            _ = host.shutdown().wait(timeout: 20)
        }

        let createdOutcome = await host.createSession(
            documentUUID: .init(high: 0xA909, low: 1)
        ).value()
        guard case let .created(created) = createdOutcome else {
            XCTFail("M9 Metal test could not create a Core session: \(createdOutcome)")
            return
        }
        let sequenceSource = CoreRGBA8Source(
            name: "cell1.png",
            documentUUID: created.documentUUID,
            sourceGeneration: 1,
            width: 8,
            height: 8,
            rgba8: Array(repeating: [UInt8(40), 0, 0, 255], count: 64).flatMap { $0 }
        )
        let lightTableSource = CoreRGBA8Source(
            name: "cell2.png",
            documentUUID: .init(high: 0xA909, low: 2),
            sourceGeneration: 2,
            width: 8,
            height: 8,
            rgba8: Array(repeating: [UInt8(0), 80, 255, 255], count: 64).flatMap { $0 }
        )
        guard case .animation = await host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .replaceSequence([sequenceSource, lightTableSource])
        ).value() else {
            XCTFail("M9 sequence source installation failed")
            return
        }
        let lightTableOutcome = await host.performAnimation(
            target: created.target,
            expectedDocumentRevision: created.documentRevision,
            command: .addLightTableItem(CoreLightTableItemSource(
                source: lightTableSource,
                opacityMilli: 650,
                displayMode: .color,
                isVisible: true
            ))
        ).value()
        guard case let .animationMutation(mutation) = lightTableOutcome,
              mutation.applied,
              mutation.state.lightTableSets.first?.items.count == 1
        else {
            XCTFail("M9 Light Table item installation failed: \(lightTableOutcome)")
            return
        }

        let route = CoreSnapshotRoute(
            session: created.target,
            view: created.primaryView,
            surface: surface
        )
        XCTAssertTrue(renderer.registerSurface(
            route: route,
            layer: metalLayer,
            drawableSize: metalLayer.drawableSize
        ))
        let snapshotOutcome = await host.buildSnapshot(route: route).value()
        guard case let .snapshot(snapshot) = snapshotOutcome else {
            XCTFail("M9 Light Table snapshot build failed: \(snapshotOutcome)")
            return
        }
        let hasVisibleRaster = try snapshot.owner.withBorrowedRenderView { view in
            view.tiles.contains { tile in tile.pixels.contains(where: { $0 != 0 }) }
        }
        XCTAssertTrue(hasVisibleRaster)
        XCTAssertEqual(renderer.submit(snapshot), .accepted)
        let rendererBecameIdle = await waitForRenderer(renderer)
        XCTAssertTrue(rendererBecameIdle)
        XCTAssertGreaterThan(renderer.metrics().presentedFrameCount, 0)
        XCTAssertEqual(snapshot.owner.ffiReleaseCount, 0)
        XCTAssertTrue(renderer.unregisterSurface(surface))
        XCTAssertEqual(snapshot.owner.ffiReleaseCount, 1)
    }

    @MainActor
    func testM8VectorFillCommitsThroughProductCanvasAndMetalStencil() async {
        _ = NSApplication.shared
        let application = ApplicationCoordinator()
        let workspaceID = WorkspaceID(
            rawValue: UUID(uuidString: "A8080000-0000-0000-0000-000000000008")!
        )
        let workspace = application.workspace(for: workspaceID)
        workspace.start()
        let hostingView = NSHostingView(
            rootView: InkpodWorkspaceScene(id: workspaceID, application: application)
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: 1_280, height: 720)
        let window = NSWindow(
            contentRect: hostingView.frame,
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentView = hostingView
        window.makeKeyAndOrderFront(nil)

        guard await waitUntil(timeout: 10, condition: {
            workspace.phase == .ready && workspace.paint != nil
        }), let canvas = await waitForCanvas(in: hostingView, timeout: 10),
              let initialContext = workspace.commandContext
        else {
            XCTFail("M8 product Canvas did not become ready")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        workspace.setCanvasVisible(true)
        XCTAssertEqual(workspace.execute(.layerNew, context: initialContext), .presentedInput)
        guard var layerDraft = workspace.pendingTreeEditor else {
            XCTFail("M8 vector layer editor was not presented")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        layerDraft.name = "Metal Vector"
        layerDraft.layerKind = .vectorColoring
        layerDraft.pixelFormat = .rgba8
        workspace.submitTreeEditor(layerDraft)
        let vectorLayerReady = await waitUntil(timeout: 10) {
            workspace.cellTree?.layers.contains(where: { $0.name == "Metal Vector" }) == true
        }
        guard vectorLayerReady,
              let layer = workspace.cellTree?.layers.first(where: { $0.name == "Metal Vector" }),
              layer.planes.contains(where: { $0.kind == .vectorFill }),
              let strokePlane = layer.planes.first(where: { $0.kind == .vectorMainLine })
        else {
            XCTFail("M8 vector fill plane was not created")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        workspace.selectNode(layerID: layer.id, planeID: strokePlane.id)
        let strokePlaneSelected = await waitUntil(timeout: 10) {
            workspace.paint?.editor.activeLayerID == layer.id
                && workspace.paint?.editor.activePlaneID == strokePlane.id
        }
        XCTAssertTrue(strokePlaneSelected)

        guard let geometryContext = workspace.commandContext else {
            XCTFail("M8 geometry target disappeared")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        XCTAssertEqual(workspace.execute(.geometryOptions, context: geometryContext), .presentedInput)
        guard case let .geometry(currentOptions)? = workspace.pendingM8Editor else {
            XCTFail("M8 geometry options were not presented")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        var fillOptions = currentOptions
        fillOptions.options.outline = false
        fillOptions.options.fill = true
        workspace.applyGeometryOptions(fillOptions)
        guard let vectorContext = workspace.commandContext,
              let before = workspace.projection
        else {
            XCTFail("M8 vector target disappeared")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        XCTAssertEqual(workspace.execute(.vectorRectangle, context: vectorContext), .started)
        let presentedBefore = application.rendererHost.metrics().presentedFrameCount
        sendMouseEvent(.leftMouseDown, to: canvas, point: NSPoint(x: 100, y: 100))
        sendMouseEvent(.leftMouseDragged, to: canvas, point: NSPoint(x: 240, y: 220))
        sendMouseEvent(.leftMouseUp, to: canvas, point: NSPoint(x: 240, y: 220))
        let vectorFillPresented = await waitUntil(timeout: 10) {
            (workspace.projection?.documentRevision ?? 0) > before.documentRevision
                && application.rendererHost.metrics().presentedFrameCount > presentedBefore
        }
        XCTAssertTrue(
            vectorFillPresented,
            "M8 vector fill did not commit/present; result=\(workspace.lastCommandResult); "
                + "projection=\(String(describing: workspace.projection)); "
                + "paint=\(String(describing: workspace.paint)); "
                + "metrics=\(application.rendererHost.metrics())"
        )

        dismantleProductCanvas(in: window)
        await application.shutdown(confirmingDirty: false)
    }

    @MainActor
    func testM7SelectionCancelUndoRedoAndHistoryUseProductCanvas() async {
        _ = NSApplication.shared
        let application = ApplicationCoordinator()
        let workspaceID = WorkspaceID(
            rawValue: UUID(uuidString: "A7070000-0000-0000-0000-000000000007")!
        )
        let workspace = application.workspace(for: workspaceID)
        workspace.start()
        let hostingView = NSHostingView(
            rootView: InkpodWorkspaceScene(id: workspaceID, application: application)
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: 1_280, height: 720)
        let window = NSWindow(
            contentRect: hostingView.frame,
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentView = hostingView
        window.makeKeyAndOrderFront(nil)

        guard await waitUntil(timeout: 10, condition: {
            workspace.phase == .ready && workspace.paint != nil
        }), let canvas = await waitForCanvas(in: hostingView, timeout: 10)
        else {
            XCTFail("M7 product Canvas did not become ready")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        workspace.setCanvasVisible(true)
        workspace.requestSnapshot()
        let initialFramePresented = await waitUntil(timeout: 10) {
            application.rendererHost.metrics().presentedFrameCount > 0
                && workspace.paint != nil
        }
        guard initialFramePresented,
              let initial = workspace.projection,
              let initialContext = workspace.commandContext
        else {
            XCTFail("M7 product Canvas did not settle its initial view target")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        XCTAssertEqual(workspace.execute(.selectionRectangle, context: initialContext), .started)
        let selectionToolReady = await waitUntil(timeout: 5) {
            workspace.paint?.editor.activeTool == .selection
                && workspace.paint?.editor.selectionOptions.shape == .rectangle
        }
        XCTAssertTrue(
            selectionToolReady,
            "selection tool did not activate; result=\(workspace.lastCommandResult); "
                + "paint=\(String(describing: workspace.paint)); "
                + "initial=\(initialContext); current=\(String(describing: workspace.commandContext))"
        )

        sendMouseEvent(.leftMouseDown, to: canvas, point: NSPoint(x: 80, y: 80))
        sendMouseEvent(.leftMouseDragged, to: canvas, point: NSPoint(x: 140, y: 140))
        canvas.cancelOperation(nil)
        XCTAssertEqual(workspace.projection?.documentRevision, initial.documentRevision)
        XCTAssertEqual(workspace.projection?.canUndo, initial.canUndo)

        sendMouseEvent(.leftMouseDown, to: canvas, point: NSPoint(x: 80, y: 80))
        sendMouseEvent(.leftMouseDragged, to: canvas, point: NSPoint(x: 180, y: 180))
        sendMouseEvent(.leftMouseUp, to: canvas, point: NSPoint(x: 180, y: 180))
        let selectionCommitted = await waitUntil(timeout: 10) {
            (workspace.projection?.documentRevision ?? 0) > initial.documentRevision
                && workspace.projection?.canUndo == true
                && workspace.history?.items.isEmpty == false
        }
        XCTAssertTrue(selectionCommitted)

        let committed = try! XCTUnwrap(workspace.projection)
        let undoContext = try! XCTUnwrap(workspace.commandContext)
        XCTAssertEqual(workspace.execute(.undo, context: undoContext), .started)
        let selectionUndone = await waitUntil(timeout: 10) {
            workspace.projection?.documentRevision != committed.documentRevision
                && workspace.projection?.canRedo == true
        }
        XCTAssertTrue(selectionUndone)
        let undone = try! XCTUnwrap(workspace.projection)
        let redoContext = try! XCTUnwrap(workspace.commandContext)
        XCTAssertEqual(workspace.execute(.redo, context: redoContext), .started)
        let selectionRedone = await waitUntil(timeout: 10) {
            workspace.projection?.documentRevision != undone.documentRevision
                && workspace.projection?.canRedo == false
        }
        XCTAssertTrue(selectionRedone)

        dismantleProductCanvas(in: window)
        await application.shutdown(confirmingDirty: false)
    }

    @MainActor
    func testM5NewCellCommandPresentsSheetAndCancelPreservesDocument() async {
        _ = NSApplication.shared
        let application = ApplicationCoordinator()
        let workspaceID = WorkspaceID(
            rawValue: UUID(uuidString: "A5050000-0000-0000-0000-000000000005")!
        )
        let workspace = application.workspace(for: workspaceID)
        workspace.start()
        guard await waitUntil(timeout: 10, condition: { workspace.phase == .ready }),
              let context = workspace.commandContext,
              let before = workspace.projection
        else {
            XCTFail("M5 workspace did not become ready")
            await application.shutdown(confirmingDirty: false)
            return
        }

        let hostingView = NSHostingView(
            rootView: InkpodWorkspaceScene(id: workspaceID, application: application)
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: 640, height: 480)
        let window = NSWindow(
            contentRect: hostingView.frame,
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentView = hostingView
        window.makeKeyAndOrderFront(nil)

        XCTAssertEqual(application.route(.fileNew, context: context), .presentedInput)
        XCTAssertNotNil(workspace.pendingNewCellDraft)
        let sheetPresented = await waitUntil(timeout: 5) { window.attachedSheet != nil }
        XCTAssertTrue(sheetPresented)

        workspace.cancelNewCell()
        let sheetDismissed = await waitUntil(timeout: 5) { window.attachedSheet == nil }
        XCTAssertTrue(sheetDismissed)
        XCTAssertNil(workspace.pendingNewCellDraft)
        XCTAssertEqual(workspace.lastCommandResult, .cancelled)
        XCTAssertEqual(workspace.projection?.target, before.target)
        XCTAssertEqual(workspace.projection?.documentRevision, before.documentRevision)
        XCTAssertEqual(workspace.projection?.canUndo, before.canUndo)
        XCTAssertEqual(workspace.projection?.canRedo, before.canRedo)
        XCTAssertEqual(workspace.projection?.isDirty, before.isDirty)

        window.orderOut(nil)
        window.contentView = nil
        await application.shutdown(confirmingDirty: false)
    }

    @MainActor
    func testM3CommandRouterStateCancelInvalidAndStaleContracts() async {
        _ = NSApplication.shared
        let application = ApplicationCoordinator()
        var settingsPresented = false
        application.installSettingsPresenter { settingsPresented = true }
        XCTAssertEqual(application.route(.shortcutEdit, context: nil), .presentedInput)
        XCTAssertTrue(settingsPresented)
        let workspace = application.workspace(for: WorkspaceID(
            rawValue: UUID(uuidString: "A3030000-0000-0000-0000-000000000003")!
        ))
        workspace.start()
        guard await waitUntil(timeout: 10, condition: { workspace.phase == .ready }),
              let initial = workspace.commandContext,
              let initialProjection = workspace.projection
        else {
            XCTFail("M3 workspace did not become ready")
            await application.shutdown(confirmingDirty: false)
            return
        }

        XCTAssertEqual(workspace.execute(.grid, context: initial), .started)
        let gridUpdated = await waitUntil(timeout: 5) {
            workspace.projection?.viewRevision != initialProjection.viewRevision
        }
        XCTAssertTrue(gridUpdated)
        let current = try! XCTUnwrap(workspace.commandContext)
        XCTAssertTrue(workspace.commandState(.grid, context: current).checked)

        XCTAssertEqual(workspace.execute(.ruler, context: initial), .started)
        let staleRejected = await waitUntil(timeout: 5) {
            workspace.lastCommandResult == .stale
        }
        XCTAssertTrue(staleRejected)
        XCTAssertFalse(workspace.commandState(.ruler, context: current).checked)

        let beforeCancel = try! XCTUnwrap(workspace.projection)
        XCTAssertEqual(workspace.execute(.zoomPercent, context: current), .presentedInput)
        XCTAssertNotNil(workspace.pendingCommandInput)
        workspace.cancelCommandInput()
        XCTAssertNil(workspace.pendingCommandInput)
        XCTAssertEqual(workspace.projection, beforeCancel)
        XCTAssertEqual(workspace.lastCommandResult, .cancelled)

        XCTAssertEqual(workspace.execute(.zoomPercent, context: current), .presentedInput)
        XCTAssertEqual(workspace.submitCommandInput(.zoomPercent(0)), .invalid)
        XCTAssertEqual(workspace.projection, beforeCancel)

        await application.shutdown(confirmingDirty: false)
    }

    @MainActor
    func testProductSceneCoversInputViewAndMetalLifecycle() async {
        _ = NSApplication.shared
        let application = ApplicationCoordinator()
        let workspaceID = WorkspaceID(
            rawValue: UUID(uuidString: "A2020000-0000-0000-0000-000000000002")!
        )
        let workspace = application.workspace(for: workspaceID)
        workspace.start()
        guard await waitUntil(timeout: 10, condition: { workspace.phase == .ready }) else {
            XCTFail("product workspace did not create its Core session")
            await application.shutdown(confirmingDirty: false)
            return
        }
        let hostingView = NSHostingView(
            rootView: InkpodWorkspaceScene(id: workspaceID, application: application)
        )
        hostingView.frame = NSRect(x: 0, y: 0, width: 640, height: 480)
        let window = NSWindow(
            contentRect: hostingView.frame,
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentView = hostingView
        window.makeKeyAndOrderFront(nil)

        guard let canvas = await waitForCanvas(in: hostingView, timeout: 10) else {
            XCTFail("product scene did not create its Metal Canvas")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        // XCTest's host process is reported as occluded even when its window is
        // ordered onscreen. Drive the same visibility transition that an
        // NSWindow occlusion notification provides in the product process.
        workspace.setCanvasVisible(true)
        workspace.requestSnapshot()
        guard await waitUntil(timeout: 10, condition: {
            canvas.accessibilityValue() as? String
                != nil && application.rendererHost.metrics().presentedFrameCount > 0
                && workspace.paint != nil
        }) else {
            XCTFail(
                "product scene did not present its first Metal frame; "
                    + "metrics=\(application.rendererHost.metrics()); "
                    + "visible=\(window.isVisible); "
                    + "occluded=\(!window.occlusionState.contains(.visible)); "
                    + "value=\(String(describing: canvas.accessibilityValue()))"
            )
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }

        let initialValue = canvas.accessibilityValue() as? String
        let initialProjection = workspace.projection!
        let initialPresentedFrame = application.rendererHost.metrics().presentedFrameCount
        sendMouseEvent(.leftMouseDown, to: canvas, point: NSPoint(x: 80, y: 80))
        guard let begunProjection = await waitForTransient(
            application.coreHost,
            target: workspace.projection!.target,
            active: true
        ) else {
            XCTFail(
                "Canvas mouseDown did not begin Core transient; "
                    + "result=\(workspace.lastCommandResult); "
                    + "paint=\(String(describing: workspace.paint)); "
                    + "projection=\(String(describing: workspace.projection))"
            )
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        XCTAssertTrue(
            begunProjection.hasActiveTransient,
            "Canvas mouseDown did not begin Core transient"
        )

        sendMouseEvent(.leftMouseDragged, to: canvas, point: NSPoint(x: 120, y: 120))
        guard let appendedProjection = await waitForTransient(
            application.coreHost,
            target: workspace.projection!.target,
            active: true
        ) else {
            XCTFail("Canvas mouseDragged did not preserve Core transient")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        XCTAssertTrue(
            appendedProjection.hasActiveTransient,
            "Canvas mouseDragged did not preserve Core transient"
        )

        sendMouseEvent(.leftMouseUp, to: canvas, point: NSPoint(x: 160, y: 160))

        let committedStrokePresented = await waitUntil(timeout: 10) {
            guard let value = canvas.accessibilityValue() as? String,
                  let projection = workspace.projection
            else {
                return false
            }
            return projection.documentRevision > initialProjection.documentRevision
                && projection.canUndo
                && value != initialValue
                && value.contains("documentRevision=")
                && application.rendererHost.metrics().presentedFrameCount
                    > initialPresentedFrame
        }
        XCTAssertTrue(
            committedStrokePresented,
            "stroke did not commit and present; initial=\(String(describing: initialValue)); "
                + "current=\(String(describing: canvas.accessibilityValue())); "
                + "projection=\(String(describing: workspace.projection)); "
                + "metrics=\(application.rendererHost.metrics())"
        )

        guard let committed = workspace.projection else {
            XCTFail("workspace projection disappeared after stroke commit")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        let committedMetrics = application.rendererHost.metrics()
        XCTAssertTrue(committed.canUndo)
        XCTAssertTrue(window.isDocumentEdited)
        XCTAssertGreaterThan(committedMetrics.uploadedTileCount, 0)

        workspace.filePanelResponseForTesting = { _ in .cancel }
        let saveAsContext = try! XCTUnwrap(workspace.commandContext)
        XCTAssertEqual(workspace.execute(.fileSaveAs, context: saveAsContext), .started)
        let savePanelCancelled = await waitUntil(timeout: 5) {
            workspace.lastCommandResult == .cancelled
        }
        XCTAssertTrue(savePanelCancelled)
        XCTAssertEqual(workspace.projection?.documentRevision, committed.documentRevision)
        XCTAssertEqual(workspace.projection?.isDirty, true)
        XCTAssertNil(workspace.documentURL)
        XCTAssertTrue(window.isDocumentEdited)
        workspace.filePanelResponseForTesting = nil

        workspace.dirtyCloseDecisionForTesting = { .cancel }
        XCTAssertFalse(
            window.delegate?.windowShouldClose?(window) ?? true,
            "dirty close must remain pending while the async decision is resolved"
        )
        await Task.yield()
        XCTAssertTrue(window.isVisible, "Cancel must keep a dirty document window open")
        XCTAssertEqual(workspace.projection?.documentRevision, committed.documentRevision)
        workspace.dirtyCloseDecisionForTesting = nil

        sendMouseEvent(.leftMouseDown, to: canvas, point: NSPoint(x: 200, y: 200))
        guard let activeProjection = await waitForTransient(
            application.coreHost,
            target: committed.target,
            active: true
        ) else {
            XCTFail("could not inspect active Canvas stroke")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        XCTAssertTrue(activeProjection.hasActiveTransient)
        canvas.cancelOperation(nil)
        guard let cancelledProjection = await waitForTransient(
            application.coreHost,
            target: committed.target,
            active: false
        ) else {
            XCTFail("could not inspect cancelled Canvas stroke")
            dismantleProductCanvas(in: window)
            await application.shutdown(confirmingDirty: false)
            return
        }
        XCTAssertFalse(cancelledProjection.hasActiveTransient)
        XCTAssertEqual(cancelledProjection.documentRevision, committed.documentRevision)

        let beforeCancelRefresh = application.rendererHost.metrics().presentedFrameCount
        workspace.requestSnapshot()
        let cancelRefreshPresented = await waitUntil(timeout: 10) {
            application.rendererHost.metrics().presentedFrameCount > beforeCancelRefresh
        }
        XCTAssertTrue(cancelRefreshPresented)
        let cancelBarrierReached = await waitForRenderer(application.rendererHost)
        XCTAssertTrue(cancelBarrierReached)
        let prePanMetrics = application.rendererHost.metrics()

        workspace.pan(deviceDX: 17, deviceDY: 9)
        let panPresented = await waitUntil(timeout: 10) {
            guard let projection = workspace.projection else { return false }
            return projection.viewRevision > committed.viewRevision
                && application.rendererHost.metrics().presentedFrameCount
                    > prePanMetrics.presentedFrameCount
        }
        XCTAssertTrue(panPresented)
        let panMetrics = application.rendererHost.metrics()
        XCTAssertEqual(panMetrics.uploadedTileCount, prePanMetrics.uploadedTileCount)
        XCTAssertGreaterThan(panMetrics.reusedTileCount, prePanMetrics.reusedTileCount)

        let panProjection = workspace.projection!
        workspace.zoom(factor: 1.25, deviceX: 320, deviceY: 240)
        let zoomPresented = await waitUntil(timeout: 10) {
            guard let projection = workspace.projection else { return false }
            return projection.viewRevision > panProjection.viewRevision
                && application.rendererHost.metrics().presentedFrameCount
                    > panMetrics.presentedFrameCount
        }
        XCTAssertTrue(zoomPresented)
        let zoomMetrics = application.rendererHost.metrics()
        XCTAssertEqual(zoomMetrics.uploadedTileCount, panMetrics.uploadedTileCount)
        XCTAssertGreaterThan(zoomMetrics.reusedTileCount, panMetrics.reusedTileCount)

        workspace.setCanvasVisible(false)
        let hiddenBarrierReached = await waitForRenderer(application.rendererHost)
        XCTAssertTrue(hiddenBarrierReached)
        let hiddenBaseline = application.rendererHost.metrics()
        workspace.requestSnapshot()
        let hiddenSnapshotRejected = await waitUntil(timeout: 10) {
            application.rendererHost.metrics().rejectedSnapshotCount
                > hiddenBaseline.rejectedSnapshotCount
        }
        XCTAssertTrue(hiddenSnapshotRejected)
        let hiddenMetrics = application.rendererHost.metrics()
        XCTAssertEqual(hiddenMetrics.presentedFrameCount, hiddenBaseline.presentedFrameCount)
        XCTAssertEqual(hiddenMetrics.hiddenDrawCount, 0)

        workspace.setCanvasVisible(true)
        let visibleFramePresented = await waitUntil(timeout: 10) {
            application.rendererHost.metrics().presentedFrameCount
                > hiddenMetrics.presentedFrameCount
        }
        XCTAssertTrue(visibleFramePresented)

        let beforeDeviceChange = application.rendererHost.metrics()
        let beforeDeviceChangeViewRevision = workspace.projection!.viewRevision
        let retainedDocumentRevision = workspace.projection!.documentRevision
        workspace.displayOrBackingChanged(CGSize(width: 1_000, height: 700))
        let deviceChangeRecovered = await waitUntil(timeout: 10) {
            let metrics = application.rendererHost.metrics()
            return metrics.deviceRebuildCount > beforeDeviceChange.deviceRebuildCount
                && metrics.presentedFrameCount > beforeDeviceChange.presentedFrameCount
                && metrics.uploadedTileCount > beforeDeviceChange.uploadedTileCount
                && (workspace.projection?.viewRevision ?? 0) > beforeDeviceChangeViewRevision
        }
        XCTAssertTrue(deviceChangeRecovered)
        XCTAssertEqual(workspace.projection?.documentRevision, retainedDocumentRevision)

        let beforeMemoryPressure = application.rendererHost.metrics()
        application.rendererHost.handleMemoryPressure()
        let memoryPurged = await waitUntil(timeout: 10) {
            application.rendererHost.metrics().memoryPressurePurgeCount
                > beforeMemoryPressure.memoryPressurePurgeCount
        }
        XCTAssertTrue(memoryPurged)
        let purgedMetrics = application.rendererHost.metrics()
        workspace.requestSnapshot()
        let postPurgeFramePresented = await waitUntil(timeout: 10) {
            let metrics = application.rendererHost.metrics()
            return metrics.presentedFrameCount > purgedMetrics.presentedFrameCount
                && metrics.uploadedTileCount > beforeMemoryPressure.uploadedTileCount
        }
        XCTAssertTrue(
            postPurgeFramePresented,
            "snapshot after memory purge was not presented; "
                + "before=\(purgedMetrics); current=\(application.rendererHost.metrics()); "
                + "result=\(workspace.lastCommandResult)"
        )
        XCTAssertEqual(workspace.projection?.documentRevision, retainedDocumentRevision)

        let beforeUndoFrame = application.rendererHost.metrics().presentedFrameCount
        workspace.undo()
        let undoPresented = await waitUntil(timeout: 10) {
            guard let projection = workspace.projection else { return false }
            return projection.canRedo
                && !projection.canUndo
                && projection.documentRevision != retainedDocumentRevision
                && application.rendererHost.metrics().presentedFrameCount > beforeUndoFrame
        }
        XCTAssertTrue(undoPresented)

        // Let the final CAMetalLayer presentation commit before unregistering
        // the renderer surface.
        try? await Task.sleep(for: .milliseconds(100))
        dismantleProductCanvas(in: window)
        await application.shutdown(confirmingDirty: false)
    }

    @MainActor
    private func waitForCanvas(
        in view: NSView,
        timeout: TimeInterval
    ) async -> CanvasHostView? {
        var found: CanvasHostView?
        _ = await waitUntil(timeout: timeout) {
            found = self.findCanvas(in: view)
            return found != nil
        }
        return found
    }

    @MainActor
    private func findCanvas(in view: NSView) -> CanvasHostView? {
        if let canvas = view as? CanvasHostView {
            return canvas
        }
        for subview in view.subviews {
            if let canvas = findCanvas(in: subview) {
                return canvas
            }
        }
        return nil
    }

    @MainActor
    private func dismantleProductCanvas(in window: NSWindow) {
        if let contentView = window.contentView,
           let canvas = findCanvas(in: contentView)
        {
            canvas.dismantle()
        }
    }

    @MainActor
    private func sendMouseEvent(
        _ type: NSEvent.EventType,
        to canvas: CanvasHostView,
        point: NSPoint
    ) {
        guard let window = canvas.window,
              let event = NSEvent.mouseEvent(
                  with: type,
                  location: canvas.convert(point, to: nil),
                  modifierFlags: [],
                  timestamp: ProcessInfo.processInfo.systemUptime,
                  windowNumber: window.windowNumber,
                  context: nil,
                  eventNumber: 1,
                  clickCount: 1,
                  pressure: 1
              )
        else {
            XCTFail("could not construct product Canvas mouse event")
            return
        }
        XCTAssertTrue(canvas.bounds.contains(point))
        XCTAssertTrue(event.window === window)
        let localPoint = canvas.convert(event.locationInWindow, from: nil)
        let rawBackingPoint = canvas.convertToBacking(localPoint)
        let backingBounds = canvas.convertToBacking(canvas.bounds)
        XCTAssertTrue(
            backingBounds.contains(rawBackingPoint),
            "mouse point did not map into Canvas backing pixels; "
                + "local=\(localPoint), backing=\(rawBackingPoint), bounds=\(backingBounds)"
        )
        let backingPoint = CanvasInputNormalizer.localDevicePoint(
            backingPoint: rawBackingPoint,
            backingBounds: backingBounds,
            isFlipped: canvas.isFlipped
        )
        XCTAssertNotNil(
            backingPoint.flatMap { point in
                CanvasInputNormalizer.sample(
                    deviceX: point.x,
                    deviceY: point.y,
                    drawableWidth: backingBounds.width,
                    drawableHeight: backingBounds.height,
                    pressure: event.pressure,
                    tilt: nil
                )
            },
            "synthetic event did not satisfy Canvas normalization; "
                + "backing=\(rawBackingPoint), bounds=\(backingBounds), "
                + "pressure=\(event.pressure)"
        )
        switch type {
        case .leftMouseDown:
            canvas.mouseDown(with: event)
        case .leftMouseDragged:
            canvas.mouseDragged(with: event)
        case .leftMouseUp:
            canvas.mouseUp(with: event)
        default:
            XCTFail("unsupported test event type")
        }
    }

    @MainActor
    private func waitUntil(
        timeout: TimeInterval,
        condition: () -> Bool
    ) async -> Bool {
        let deadline = Date(timeIntervalSinceNow: timeout)
        repeat {
            if condition() {
                return true
            }
            try? await Task.sleep(for: .milliseconds(10))
        } while Date() < deadline
        return condition()
    }

    @MainActor
    private func waitForRenderer(_ renderer: MetalRendererHost) async -> Bool {
        await Task.detached {
            renderer.waitUntilIdle(timeout: 10)
        }.value
    }

    @MainActor
    private func waitForTransient(
        _ host: CoreHost,
        target: CoreSessionTarget,
        active: Bool,
        timeout: TimeInterval = 5
    ) async -> CoreSessionProjection? {
        let deadline = Date(timeIntervalSinceNow: timeout)
        repeat {
            let outcome = await host.inspectSession(target).value()
            if case let .inspected(projection) = outcome,
               projection.hasActiveTransient == active
            {
                return projection
            }
            try? await Task.sleep(for: .milliseconds(10))
        } while Date() < deadline
        return nil
    }
}
