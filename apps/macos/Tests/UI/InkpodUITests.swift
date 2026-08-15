import XCTest

final class InkpodUITests: XCTestCase {
    @MainActor
    func testM10BatchWindowRunsDryRunAndKeepsReportVisible() throws {
        let application = XCUIApplication()
        application.launch()

        let englishWindowMenu = application.menuBars.menuBarItems["Window"].firstMatch
        let japaneseWindowMenu = application.menuBars.menuBarItems["ウインドウ"].firstMatch
        let windowMenu = englishWindowMenu.exists ? englishWindowMenu : japaneseWindowMenu
        XCTAssertTrue(windowMenu.waitForExistence(timeout: 10))
        windowMenu.click()
        let identifiedBatch = application.menuItems["inkpod.command.41901"].firstMatch
        let englishBatch = application.menuItems["Batch"].firstMatch
        let japaneseBatch = application.menuItems["バッチ"].firstMatch
        let batch = identifiedBatch.exists
            ? identifiedBatch : (englishBatch.exists ? englishBatch : japaneseBatch)
        XCTAssertTrue(batch.waitForExistence(timeout: 5))
        batch.click()

        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.batch.window"]
                .waitForExistence(timeout: 5)
        )
        let add = application.descendants(matching: .any)["inkpod.batch.add-operation"].firstMatch
        XCTAssertTrue(add.waitForExistence(timeout: 5))
        add.click()
        let invert = application.menuItems["inkpod.batch.command.42025"]
        XCTAssertTrue(invert.waitForExistence(timeout: 5))
        invert.click()

        let dryRun = application.descendants(matching: .any)["inkpod.batch.command.41917"].firstMatch
        XCTAssertTrue(dryRun.waitForExistence(timeout: 5))
        dryRun.click()
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.batch.report"]
                .waitForExistence(timeout: 10)
        )
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.batch.command.41922"].firstMatch.exists
        )
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.batch.command.42012"].firstMatch.exists
        )
    }

    @MainActor
    func testM9SequenceLightTableAndCutCreationAreNativeSurfaces() throws {
        let application = XCUIApplication()
        application.launch()

        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m9.sequence-timeline"]
                .waitForExistence(timeout: 10)
        )
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m9.animation-inspector"]
                .waitForExistence(timeout: 5)
        )
        let animationMenu = application.menuBars.menuBarItems["Animation"].exists
            ? application.menuBars.menuBarItems["Animation"]
            : application.menuBars.menuBarItems["アニメーション"]
        XCTAssertTrue(animationMenu.waitForExistence(timeout: 5))

        let fileMenu = application.menuBars.menuBarItems["File"].exists
            ? application.menuBars.menuBarItems["File"]
            : application.menuBars.menuBarItems["ファイル"]
        XCTAssertTrue(fileMenu.waitForExistence(timeout: 5))
        fileMenu.click()
        let newCut = application.menuItems["New Cut…"].exists
            ? application.menuItems["New Cut…"] : application.menuItems["新規Cut…"]
        XCTAssertTrue(newCut.waitForExistence(timeout: 5))
        newCut.click()
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m9.editor-sheet"]
                .waitForExistence(timeout: 5)
        )
        let cancel = application.buttons["Cancel"].exists
            ? application.buttons["Cancel"] : application.buttons["キャンセル"]
        cancel.click()
    }

    @MainActor
    func testM8InspectorAndFilterPreviewSheetAreNativeSurfaces() throws {
        let application = XCUIApplication()
        application.launch()

        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m8.inspector"]
                .waitForExistence(timeout: 10)
        )
        let imageMenu = application.menuBars.menuBarItems["Image"].exists
            ? application.menuBars.menuBarItems["Image"]
            : application.menuBars.menuBarItems["画像"]
        XCTAssertTrue(imageMenu.waitForExistence(timeout: 5))
        imageMenu.click()
        let filterMenu = application.menuItems["Filters"].exists
            ? application.menuItems["Filters"] : application.menuItems["フィルター"]
        XCTAssertTrue(filterMenu.waitForExistence(timeout: 5))
        application.typeKey(.escape, modifierFlags: [])
        let invert = application.buttons["inkpod.command.41002"].firstMatch
        XCTAssertTrue(invert.waitForExistence(timeout: 5))
        invert.click()
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m8.filter-sheet"]
                .waitForExistence(timeout: 5)
        )
        let englishCancel = application.buttons["Cancel"].firstMatch
        let japaneseCancel = application.buttons["キャンセル"].firstMatch
        let cancel = englishCancel.exists ? englishCancel : japaneseCancel
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.click()
        XCTAssertFalse(
            application.descendants(matching: .any)["inkpod.m8.filter-sheet"]
                .waitForExistence(timeout: 2)
        )
    }

    @MainActor
    func testM7SelectionUndoRedoAndHistorySurfaces() throws {
        let application = XCUIApplication()
        application.launch()

        let canvas = application.descendants(matching: .any)["inkpod.canvas"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 10))
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m7.history-inspector"]
                .waitForExistence(timeout: 5)
        )
        let identifiedRectangle = application.buttons["inkpod.command.40806"].firstMatch
        let englishRectangle = application.buttons["Rectangle Selection"].firstMatch
        let japaneseRectangle = application.buttons["矩形選択"].firstMatch
        let rectangle = identifiedRectangle.exists
            ? identifiedRectangle
            : (englishRectangle.exists ? englishRectangle : japaneseRectangle)
        XCTAssertTrue(rectangle.waitForExistence(timeout: 5))
        rectangle.click()
        let selectionToolReady = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                (object as? XCUIElement)?.isSelected == true
            },
            object: rectangle
        )
        XCTAssertEqual(XCTWaiter.wait(for: [selectionToolReady], timeout: 5), .completed)

        let before = try XCTUnwrap(canvas.value as? String)
        let start = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.3))
        let end = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.55, dy: 0.55))
        start.press(
            forDuration: 0.05,
            thenDragTo: end,
            withVelocity: .slow,
            thenHoldForDuration: 0
        )
        let selected = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else { return false }
                return value != before && value.contains("documentRevision=")
            },
            object: canvas
        )
        XCTAssertEqual(XCTWaiter.wait(for: [selected], timeout: 10), .completed)
        let committed = try XCTUnwrap(canvas.value as? String)

        application.typeKey("z", modifierFlags: .command)
        let undone = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else { return false }
                return value != committed
            },
            object: canvas
        )
        XCTAssertEqual(XCTWaiter.wait(for: [undone], timeout: 10), .completed)
        let undoneValue = try XCTUnwrap(canvas.value as? String)

        application.typeKey("z", modifierFlags: [.command, .shift])
        let redone = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else { return false }
                return value != undoneValue && value.contains("documentRevision=")
            },
            object: canvas
        )
        XCTAssertEqual(XCTWaiter.wait(for: [redone], timeout: 10), .completed)
    }

    @MainActor
    func testM6PaintAndColorSurfacesRouteBrushToCanvas() throws {
        let application = XCUIApplication()
        application.launch()

        let canvas = application.descendants(matching: .any)["inkpod.canvas"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 10))
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m6.tool-sidebar"]
                .waitForExistence(timeout: 5)
        )
        XCTAssertTrue(
            application.descendants(matching: .any)["inkpod.m6.color-inspector"]
                .waitForExistence(timeout: 5)
        )
        for identifier in [
            "inkpod.m6.color-section",
            "inkpod.m6.palette-section",
            "inkpod.m6.chart-section",
            "inkpod.m6.locator-section",
        ] {
            XCTAssertTrue(
                application.descendants(matching: .any)[identifier]
                    .waitForExistence(timeout: 5),
                "missing M6 surface \(identifier)"
            )
        }

        let brush = application.buttons["inkpod.command.40302"].firstMatch
        XCTAssertTrue(brush.waitForExistence(timeout: 5))
        brush.click()

        let before = try XCTUnwrap(canvas.value as? String)
        let start = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.4, dy: 0.4))
        let end = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.45, dy: 0.45))
        start.press(
            forDuration: 0.05,
            thenDragTo: end,
            withVelocity: .slow,
            thenHoldForDuration: 0
        )
        let revisionChanged = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else { return false }
                return value != before && value.contains("documentRevision=")
            },
            object: canvas
        )
        XCTAssertEqual(XCTWaiter.wait(for: [revisionChanged], timeout: 10), .completed)
    }

    @MainActor
    func testProductCanvasCreatesAndCommitsCoreStroke() throws {
        let application = XCUIApplication()
        if ProcessInfo.processInfo.environment["INKPOD_ENABLE_METAL_VALIDATION"] == "1" {
            application.launchEnvironment["MTL_DEBUG_LAYER"] = "1"
            application.launchEnvironment["MTL_DEBUG_LAYER_ERROR_MODE"] = "assert"
        }
        application.launch()

        let canvas = application.descendants(matching: .any)["inkpod.canvas"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 10))

        let firstFrame = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else {
                    return false
                }
                return !value.contains("presentedFrames=0")
                    && value.contains("presentedFrames=")
            },
            object: canvas
        )
        XCTAssertEqual(XCTWaiter.wait(for: [firstFrame], timeout: 10), .completed)

        let initialValue = try XCTUnwrap(canvas.value as? String)
        XCTAssertTrue(initialValue.contains("documentRevision="))

        let start = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.25, dy: 0.25))
        let end = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        start.press(forDuration: 0.05, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)

        let revisionChanged = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else {
                    return false
                }
                return value != initialValue && value.contains("documentRevision=")
            },
            object: canvas
        )
        XCTAssertEqual(XCTWaiter.wait(for: [revisionChanged], timeout: 10), .completed)
    }

    @MainActor
    func testSettingsShortcutEditRoutesBackToExactWorkspace() throws {
        let application = XCUIApplication()
        application.launch()

        let canvas = application.descendants(matching: .any)["inkpod.canvas"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 10))
        application.typeKey(",", modifierFlags: .command)

        let search = application.textFields["inkpod.settings.search"]
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        let record = application.buttons["inkpod.shortcut.record.40201"]
        search.click()
        search.typeText(search.placeholderValue == "コマンドを検索" ? "拡大" : "Zoom In")
        guard record.waitForExistence(timeout: 5) else {
            XCTFail("Zoom In shortcut row was not filtered into view")
            return
        }
        let recordHittable = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "hittable == true"),
            object: record
        )
        XCTAssertEqual(XCTWaiter.wait(for: [recordHittable], timeout: 5), .completed)
        record.click()
        application.typeKey("2", modifierFlags: .command)
        let apply = application.buttons["inkpod.shortcut.apply"]
        XCTAssertTrue(apply.waitForExistence(timeout: 5))
        apply.click()

        application.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(canvas.waitForExistence(timeout: 5))
        let zoomButton = application.buttons["inkpod.command.40201"]
        XCTAssertTrue(zoomButton.waitForExistence(timeout: 5))
        zoomButton.click()
        let before = try XCTUnwrap(canvas.value as? String)
        application.typeKey("2", modifierFlags: .command)
        let viewChanged = XCTNSPredicateExpectation(
            predicate: NSPredicate { object, _ in
                guard let element = object as? XCUIElement,
                      let value = element.value as? String
                else {
                    return false
                }
                return value != before && value.contains("viewRevision=")
            },
            object: canvas
        )
        XCTAssertEqual(XCTWaiter.wait(for: [viewChanged], timeout: 10), .completed)

        application.typeKey(",", modifierFlags: .command)
        let reset = application.buttons["inkpod.shortcut.reset"]
        XCTAssertTrue(reset.waitForExistence(timeout: 5))
        reset.click()
    }

    @MainActor
    func testM4SaveCommandPresentsSandboxSavePanelForDirtyDocument() throws {
        let application = XCUIApplication()
        application.launch()

        let canvas = application.descendants(matching: .any)["inkpod.canvas"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 10))
        let start = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.3, dy: 0.3))
        let end = canvas.coordinate(withNormalizedOffset: CGVector(dx: 0.35, dy: 0.35))
        start.press(
            forDuration: 0.05,
            thenDragTo: end,
            withVelocity: .slow,
            thenHoldForDuration: 0
        )

        application.typeKey("s", modifierFlags: .command)
        // A sandboxed NSSavePanel is hosted outside the app's AX hierarchy.
        // Its application-modal state is observable through the disabled app.
        let panelBegan = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == false"),
            object: application
        )
        XCTAssertEqual(XCTWaiter.wait(for: [panelBegan], timeout: 5), .completed)
        XCTAssertTrue(canvas.waitForExistence(timeout: 5))
        application.terminate()
    }

}
