import XCTest

final class InkpodUITests: XCTestCase {
    @MainActor
    func testWorkspaceMenuIsDistinctFromSystemWindowMenuAndAboutHasProductCopy() throws {
        let application = XCUIApplication()
        application.launch()

        let menuBarItems = application.menuBars.menuBarItems
        let workspaceCount = menuBarItems.matching(identifier: "Workspace").count
            + menuBarItems.matching(identifier: "ワークスペース").count
        let systemWindowCount = menuBarItems.matching(identifier: "Window").count
            + menuBarItems.matching(identifier: "ウインドウ").count
        XCTAssertEqual(workspaceCount, 1)
        XCTAssertEqual(systemWindowCount, 1)

        let japaneseWorkspace = menuBarItems["ワークスペース"].firstMatch.exists
        let appMenu = menuBarItems["Inkpod"].firstMatch
        XCTAssertTrue(appMenu.waitForExistence(timeout: 10))
        appMenu.click()
        let englishAbout = application.menuItems["About Inkpod"].firstMatch
        let japaneseAbout = application.menuItems["Inkpodについて"].firstMatch
        let about = englishAbout.exists ? englishAbout : japaneseAbout
        XCTAssertTrue(about.waitForExistence(timeout: 5))
        about.click()

        let description = japaneseWorkspace
            ? "Inkpod は、ベクターベースのストロークによる非破壊編集に対応した、GPU アクセラレーション型ペイントエンジンです。"
            : "Inkpod is a GPU-accelerated painting engine with vector-based strokes for non-destructive editing."
        XCTAssertTrue(
            application.descendants(matching: .any)[description]
                .waitForExistence(timeout: 5)
        )
        XCTAssertTrue(
            application.descendants(matching: .any)["© Shuichi Kurabayashi"]
                .waitForExistence(timeout: 5)
        )
    }

    @MainActor
    func testM10BatchWindowRunsDryRunAndKeepsReportVisible() throws {
        let application = XCUIApplication()
        application.launch()

        let englishWorkspaceMenu = application.menuBars.menuBarItems["Workspace"].firstMatch
        let japaneseWorkspaceMenu = application.menuBars.menuBarItems["ワークスペース"].firstMatch
        let workspaceMenu = englishWorkspaceMenu.exists
            ? englishWorkspaceMenu : japaneseWorkspaceMenu
        XCTAssertTrue(workspaceMenu.waitForExistence(timeout: 10))
        workspaceMenu.click()
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

        selectInspectorTab("animation", in: application)
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

        selectInspectorTab("vectorAnnotationGuides", in: application)
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
        selectInspectorTab("history", in: application)
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
            application.descendants(matching: .any)["inkpod.m11.tool-surface"]
                .waitForExistence(timeout: 5)
        )
        selectInspectorTab("color", in: application)
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
    func testM11WorkspaceChromeUsesOneInspectorAndSharedToggleRoutes() throws {
        let application = XCUIApplication()
        application.launch()

        let canvas = application.descendants(matching: .any)["inkpod.canvas"]
        let inspector = application.descendants(matching: .any)["inkpod.m11.inspector"]
        let toolSurface = application.descendants(matching: .any)["inkpod.m11.tool-surface"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 10))
        XCTAssertTrue(inspector.waitForExistence(timeout: 5))
        XCTAssertTrue(toolSurface.waitForExistence(timeout: 5))
        XCTAssertEqual(
            application.descendants(matching: .any).matching(
                identifier: "inkpod.m11.inspector"
            ).count,
            1
        )

        for section in [
            "layerPlane", "color", "history", "vectorAnnotationGuides", "animation",
        ] {
            selectInspectorTab(section, in: application)
        }

        application.typeKey("i", modifierFlags: [.command, .control])
        XCTAssertFalse(inspector.waitForExistence(timeout: 2))
        application.typeKey("i", modifierFlags: [.command, .control])
        XCTAssertTrue(inspector.waitForExistence(timeout: 5))
        XCTAssertTrue(
            application.buttons["inkpod.m11.inspector-tab.animation"].isSelected
        )

        let toolToggle = application.buttons["inkpod.command.41900"].firstMatch
        XCTAssertTrue(toolToggle.waitForExistence(timeout: 5))
        toolToggle.click()
        XCTAssertFalse(toolSurface.waitForExistence(timeout: 2))

        let workspaceMenu = application.menuBars.menuBarItems["Workspace"].firstMatch.exists
            ? application.menuBars.menuBarItems["Workspace"].firstMatch
            : application.menuBars.menuBarItems["ワークスペース"].firstMatch
        XCTAssertTrue(workspaceMenu.waitForExistence(timeout: 5))
        workspaceMenu.click()
        let identifiedMenuToolToggle = application.menuItems["inkpod.command.41900"].firstMatch
        let englishMenuToolToggle = application.menuItems["Tool Palette"].firstMatch
        let japaneseMenuToolToggle = application.menuItems["ツールパレット"].firstMatch
        let menuToolToggle = identifiedMenuToolToggle.exists
            ? identifiedMenuToolToggle
            : (englishMenuToolToggle.exists ? englishMenuToolToggle : japaneseMenuToolToggle)
        XCTAssertTrue(menuToolToggle.waitForExistence(timeout: 5))
        menuToolToggle.click()
        XCTAssertTrue(toolSurface.waitForExistence(timeout: 5))

        try application.performAccessibilityAudit { issue in
            self.isKnownMacOSFrameworkAuditFalsePositive(issue)
        }

        let fillOptions = application.descendants(matching: .any)[
            "inkpod.tool-options.1001"
        ]
        XCTAssertTrue(fillOptions.waitForExistence(timeout: 5))
        fillOptions.click()
        let optionsPopover = application.descendants(matching: .any)[
            "inkpod.tool-options.popover"
        ]
        XCTAssertTrue(optionsPopover.waitForExistence(timeout: 5))
        let pin = application.descendants(matching: .any)["inkpod.tool-options.pin"]
        XCTAssertTrue(pin.waitForExistence(timeout: 5))
        pin.click()

        let pencil = toolSurface.descendants(matching: .button)[
            "inkpod.command.40301"
        ]
        XCTAssertTrue(pencil.waitForExistence(timeout: 5))
        pencil.click()
        XCTAssertTrue(optionsPopover.waitForExistence(timeout: 2))

        pin.click()
        let close = application.descendants(matching: .any)["inkpod.tool-options.close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        close.click()
        XCTAssertFalse(optionsPopover.waitForExistence(timeout: 2))
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
        XCTAssertFalse(application.buttons["inkpod.command.40101"].exists)
        XCTAssertFalse(application.buttons["inkpod.command.40201"].exists)
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
    func testM4FileMenuExposesSaveCommands() throws {
        let application = XCUIApplication()
        application.launch()

        let canvas = application.descendants(matching: .any)["inkpod.canvas"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 10))

        let fileMenu = application.menuBars.menuBarItems["File"].exists
            ? application.menuBars.menuBarItems["File"]
            : application.menuBars.menuBarItems["ファイル"]
        XCTAssertTrue(fileMenu.waitForExistence(timeout: 5))
        fileMenu.click()

        let englishSave = application.menuItems["Save"].firstMatch
        let japaneseSave = application.menuItems["保存"].firstMatch
        let save = englishSave.exists ? englishSave : japaneseSave
        let englishSaveAs = application.menuItems["Save As…"].firstMatch
        let japaneseSaveAs = application.menuItems["名前を付けて保存…"].firstMatch
        let saveAs = englishSaveAs.exists ? englishSaveAs : japaneseSaveAs
        XCTAssertTrue(save.waitForExistence(timeout: 5))
        XCTAssertTrue(saveAs.waitForExistence(timeout: 5))
        XCTAssertTrue(save.isEnabled)
        XCTAssertTrue(saveAs.isEnabled)

        save.click()
        // A sandboxed NSSavePanel is hosted outside the app's AX hierarchy.
        let panelBegan = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "enabled == false"),
            object: application
        )
        XCTAssertEqual(XCTWaiter.wait(for: [panelBegan], timeout: 5), .completed)
        application.terminate()
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

    @MainActor
    private func selectInspectorTab(_ section: String, in application: XCUIApplication) {
        let tab = application.buttons["inkpod.m11.inspector-tab.\(section)"].firstMatch
        XCTAssertTrue(tab.waitForExistence(timeout: 5), "missing inspector tab \(section)")
        tab.click()
        XCTAssertTrue(tab.isSelected, "inspector tab \(section) did not become selected")
    }

    @MainActor
    private func isKnownMacOSFrameworkAuditFalsePositive(
        _ issue: XCUIAccessibilityAuditIssue
    ) -> Bool {
        guard let element = issue.element else { return false }
        if issue.auditType == .contrast {
            // The system-owned window title is white on the dark titlebar, but
            // macOS 26 reports it as having no measurable background after the
            // primary-action toolbar is absent. The audit exposes the native
            // title as an empty, full-width static-text container rather than
            // the visible "Inkpod" glyph element.
            let frame = element.frame
            return element.elementType == .staticText
                && element.label.isEmpty
                && element.identifier.isEmpty
                && frame.width > 600
                && frame.height <= 64
                && frame.minY < 100
        }
        if issue.auditType == .sufficientElementDescription {
            // SwiftUI and AppKit add identifier-less container groups (and the
            // system Touch Bar proxy) around labeled, independently audited controls.
            return element.elementType == .touchBar
                || (element.elementType == .group && element.identifier.isEmpty)
        }
        if issue.auditType == .parentChild {
            // AppKit contributes a tiny identifier-less full-screen control group.
            // Keep the exception bounded so product-owned groups remain audited.
            let frame = element.frame
            return element.elementType == .group
                && element.identifier.isEmpty
                && frame.width <= 16
                && frame.height <= 16
        }
        if issue.auditType == .action {
            // The Picker exposes an adjustable accessibility action, but XCTest
            // still reports its exact pop-up proxy as having no default action.
            return element.elementType == .popUpButton
                && element.identifier == "inkpod.m9.light-table-set"
        }
        return false
    }

}
