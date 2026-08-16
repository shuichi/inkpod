import AppKit
import SwiftUI

public struct InkpodCommands: Commands {
    @ObservedObject private var application: ApplicationCoordinator
    @ObservedObject private var shortcuts: ShortcutController
    @ObservedObject private var language: AppLanguageController
    @FocusedValue(\.inkpodCommandTarget) private var context

    public init(application: ApplicationCoordinator) {
        self.application = application
        shortcuts = application.shortcutController
        language = application.languageController
    }

    public var body: some Commands {
        applicationChrome
        CommandGroup(replacing: .newItem) {
            item(.fileNew)
            item(.fileNewCut)
            item(.fileOpen)
            Menu(language.commandLabel(.fileOpenRecent)) {
                if application.recentURLs.isEmpty {
                    Text(language.text("file.recent.empty"))
                } else {
                    ForEach(application.recentURLs, id: \.self) { url in
                        Button(url.lastPathComponent) {
                            application.openRecent(url, context: resolvedCommandContext)
                        }
                        .help(url.path)
                    }
                }
            }
        }
        CommandGroup(replacing: .saveItem) {
            item(.fileSave)
            item(.fileSaveAs)
            Divider()
            item(.fileRevert)
            item(.fileRevertPartial)
        }
        CommandGroup(replacing: .importExport) {
            item(.fileImportRaster)
            item(.fileExportRaster)
            item(.fileExportInstructionRaster)
        }
        CommandGroup(after: .saveItem) {
            Menu(language.text("menu.cut")) {
                item(.cutProperties)
                item(.cutSave)
                item(.cutUndo)
                item(.cutRedo)
                Divider()
                item(.cutSequenceAdd)
                item(.cutSequenceRemove)
                item(.cutSequenceMoveUp)
                item(.cutSequenceMoveDown)
                item(.cutSequenceRenumber)
            }
            Divider()
            item(.fileAutosaveNow)
            item(.fileOpenRecovery)
            item(.fileRestorePrevious)
            item(.fileCompactCopy)
            item(.fileSequenceAutosave)
            Divider()
            item(.documentClose)
        }
        CommandGroup(replacing: .undoRedo) {
            item(.undo)
            item(.redo)
            Menu(language.text("menu.history")) {
                item(.historyBack)
                item(.historyForward)
                Divider()
                historyItems
            }
        }
        CommandGroup(replacing: .pasteboard) {
            item(.editCut)
            item(.editCopy)
            item(.editPaste)
            item(.editPasteSelected)
            item(.editPasteConverted)
            Divider()
            item(.editMirrorHorizontal)
            item(.floatingTransform)
            item(.floatingCommit)
            item(.floatingCancel)
        }
        CommandMenu(language.text("menu.view")) {
            item(.viewNew)
            item(.viewClose)
            Divider()
            item(.tabNext)
            item(.tabPrevious)
            item(.tabMoveLeft)
            item(.tabMoveRight)
            Divider()
            item(.editorSplitRight)
            item(.editorSplitDown)
            item(.editorMoveOtherGroup)
            item(.editorNewViewOtherGroup)
            item(.editorGroupNext)
            item(.editorGroupClose)
            Divider()
            item(.viewMoveNextWindow)
            item(.viewDuplicateNextWindow)
            item(.viewMoveNewWindow)
            item(.viewDuplicateNewWindow)
            Divider()
            item(.zoomIn)
            item(.zoomOut)
            item(.fit)
            item(.oneToOne)
            item(.zoomPercent)
            item(.boxZoom)
            Divider()
            item(.flipHorizontal)
            item(.flipVertical)
            Divider()
            item(.ruler)
            item(.guides)
            item(.grid)
            item(.transparent)
            Divider()
            item(.snapGuides)
            item(.snapGrid)
            Divider()
            item(.guideVertical)
            item(.guideHorizontal)
            item(.guideMove)
            item(.guideDeleteAll)
            item(.gridSettings)
            Divider()
            item(.viewVectorAntialias)
            item(.viewVectorCenterline)
            item(.viewVectorCenterlineOnly)
            item(.viewVectorEndpoints)
        }
        CommandMenu(language.text("menu.cell")) {
            item(.cellPaperSettings)
            Menu(language.text("menu.cell.frames")) {
                item(.cellFrameHundred)
                item(.cellFrameReference)
                item(.cellFrameDrawing)
                item(.cellFrameSafe)
                item(.cellMargins)
            }
            Divider()
            item(.cellMirrorVertical)
            item(.cellRotateLeft)
            item(.cellRotateRight)
            item(.cellImageSize)
            item(.cellResolution)
            item(.cellFitCaptureFrame)
            Divider()
            Menu(language.text("menu.cell.shootingFrame")) {
                item(.cellShootingFrameProperties)
                item(.cellShootingFrameEditHandles)
                item(.cellShootingFrameDelete)
            }
            Menu(language.text("menu.cell.vanishingPoint")) {
                item(.cellVanishingPointProperties)
                item(.cellVanishingPointEditHandles)
                item(.cellVanishingPointDeleteAll)
            }
            Menu(language.text("menu.cell.annotation")) {
                item(.annotationAddText)
                item(.annotationEditText)
                item(.annotationDrawInstruction)
                item(.annotationSelectPrevious)
                item(.annotationSelectNext)
                item(.annotationMoveLeft)
                item(.annotationMoveRight)
                item(.annotationDelete)
            }
            Divider()
            Menu(language.text("menu.layer")) {
                item(.layerNew)
                item(.layerDuplicate)
                item(.layerDelete)
                item(.layerDeleteHidden)
                Divider()
                item(.layerMoveTop)
                item(.layerMoveUp)
                item(.layerMoveDown)
                Divider()
                item(.layerToggleVisible)
                item(.layerToggleEditable)
                item(.layerOpacity)
                item(.layerConvert)
                item(.layerMerge)
                item(.layerProperties)
            }
            Menu(language.text("menu.plane")) {
                item(.planeMainLine)
                item(.planeColor)
                Divider()
                item(.planeNew)
                item(.planeDuplicate)
                item(.planeDelete)
                item(.planeMoveUp)
                item(.planeMoveDown)
                Divider()
                item(.planeToggleVisible)
                item(.planeToggleEditable)
                item(.planeOpacity)
                item(.planeConvert)
                item(.planeMerge)
                item(.planeProperties)
            }
        }
        additionalMenus
    }

    @CommandsBuilder
    private var applicationChrome: some Commands {
        InspectorCommands()
        CommandGroup(replacing: .appInfo) {
            item(.helpAbout)
        }
        CommandGroup(replacing: .appTermination) {
            item(.appExit)
        }
        CommandGroup(replacing: .help) {
            item(.helpManual)
            item(.helpFileFormat)
            item(.helpWebPage)
            Divider()
            item(.helpAcknowledgements)
        }
    }

    @CommandsBuilder
    private var m6Menus: some Commands {
        CommandMenu(language.text("menu.tools")) {
            item(.toolPencil)
            item(.toolBrush)
            item(.toolEraser)
            item(.toolFill)
            item(.toolEyedropper)
            Divider()
            item(.toolFillOptions)
            item(.toolClosedFill)
            item(.toolFillExtension)
            Divider()
            item(.toolColorReplaceTarget)
            Menu(language.text("menu.tools.colorReplaceRegion")) {
                item(.toolColorReplacePen)
                item(.toolColorReplaceRectangle)
                item(.toolColorReplacePolyline)
                item(.toolColorReplaceLasso)
                item(.toolColorReplaceAll)
            }
            Divider()
            Menu(language.text("menu.tools.vector")) {
                item(.vectorLine)
                item(.vectorCurve)
                item(.vectorRectangle)
                item(.vectorEllipse)
                item(.vectorPolyline)
                item(.vectorPolygon)
                item(.geometryOptions)
                Divider()
                item(.vectorEraser)
                item(.vectorErasePartial)
                item(.vectorEraseIntersection)
                item(.vectorEraseWhole)
                item(.vectorConnect)
                item(.vectorWidth)
                Divider()
                item(.vectorSelectCut)
                item(.vectorSelectTouch)
                item(.vectorSelectContained)
                item(.vectorSelectLine)
                item(.vectorSelectWholeLine)
                item(.vectorSelectIntersection)
                item(.vectorSelectFillBoundary)
                item(.vectorSelectFill)
                Divider()
                item(.vectorRasterize)
                item(.vectorVectorize)
            }
        }
        CommandMenu(language.text("menu.color")) {
            item(.colorChoose)
            item(.colorEditor)
            item(.colorPin)
            Divider()
            item(.colorCheckOff)
            item(.colorCheckLegacy)
            item(.colorCheckNative)
            Divider()
            Menu(language.text("menu.color.eyedropperSource")) {
                item(.colorSourceTopmost)
                item(.colorSourceSelected)
                item(.colorSourceComposite)
                item(.colorSourceLightTable)
            }
            Menu(language.text("menu.color.palette")) {
                item(.paletteRegister)
                item(.paletteDelete)
                item(.paletteClear)
                item(.paletteNextGroup)
                Divider()
                item(.paletteLoad)
                item(.paletteSave)
            }
            Menu(language.text("menu.color.chart")) {
                item(.chartGenerate)
                item(.chartSearch)
                item(.chartNext)
                item(.chartLock)
                Divider()
                item(.chartCut)
                item(.chartCopy)
                item(.chartPaste)
                item(.chartRename)
                Divider()
                item(.chartLoad)
                item(.chartSave)
                item(.chartNextPage)
            }
        }
        CommandMenu(language.text("menu.selection")) {
            item(.selectionAll)
            item(.selectionInvert)
            item(.selectionExpand)
            item(.selectionShrink)
            item(.selectionClear)
            Divider()
            Menu(language.text("menu.selection.shape")) {
                item(.selectionRectangle)
                item(.selectionEllipse)
                item(.selectionLasso)
                item(.selectionPolyline)
                item(.selectionTrace)
                item(.selectionWand)
            }
            Menu(language.text("menu.selection.mode")) {
                item(.selectionModeNew)
                item(.selectionModeAdd)
                item(.selectionModeSubtract)
                item(.selectionModeIntersect)
            }
            Divider()
            item(.selectionColor)
            item(.selectionColorDifferent)
            item(.selectionColorAdd)
            Divider()
            item(.selectionToLayer)
            item(.selectionFromLayer)
            item(.selectionLayerAdd)
            item(.selectionLayerSubtract)
            Divider()
            item(.selectionOptions)
            item(.selectionOutputColorGuard)
        }
    }

    @CommandsBuilder
    private var additionalMenus: some Commands {
        m6Menus
        m8ImageMenu
        m9AnimationMenu
        workspaceWindowMenu
    }

    @CommandsBuilder
    private var m9AnimationMenu: some Commands {
        CommandMenu(language.text("menu.animation")) {
            Menu(language.text("menu.sequence")) {
                item(.sequenceImport)
                item(.sequenceExport)
                Divider()
                item(.sequencePrevious)
                item(.sequenceNext)
                item(.sequenceGoto)
                item(.sequenceWrapEndpoints)
            }
            Menu(language.text("menu.lightTable")) {
                item(.lightTableSetNew)
                item(.lightTableSetDuplicate)
                item(.lightTableSetDelete)
                item(.lightTableSetRename)
                item(.lightTableSetUp)
                item(.lightTableSetDown)
                item(.lightTableGlobalOpacity)
                Divider()
                item(.lightTableItemAdd)
                item(.lightTableItemReload)
                item(.lightTableItemDelete)
                item(.lightTableItemUp)
                item(.lightTableItemDown)
                item(.lightTableItemProperties)
                item(.lightTableItemSample)
                item(.lightTableItemSwap)
                item(.lightTableItemMove)
                Divider()
                item(.lightTableBulkPrevious)
                item(.lightTableBulkNext)
                item(.lightTableBulkBoth)
            }
            Menu(language.text("menu.subpalette")) {
                item(.subpaletteSet)
                item(.subpaletteSample)
            }
            Menu(language.text("menu.motion")) {
                item(.motionStart)
                item(.motionPause)
                item(.motionPrevious)
                item(.motionNext)
                item(.motionStop)
                item(.motionFirst)
                item(.motionLast)
                Divider()
                item(.motionFPS30)
                item(.motionFPS25)
                item(.motionFPS24)
                item(.motionFPS12)
                item(.motionFPS10)
                item(.motionFPS8)
            }
        }
    }

    @CommandsBuilder
    private var m8ImageMenu: some Commands {
        CommandMenu(language.text("menu.image")) {
            Menu(language.text("menu.image.filter")) {
                item(.filterLast)
                Divider()
                item(.filterInvert)
                item(.filterBlurWeak)
                item(.filterBlurStrong)
                item(.filterSharpenWeak)
                item(.filterSharpenStrong)
                item(.filterGaussian)
                item(.filterAutoContrast)
                item(.filterBrightness)
                item(.filterToneCurve)
                item(.filterLevels)
                item(.filterHSV)
                item(.filterColorBalance)
                item(.filterUnsharp)
            }
            Menu(language.text("menu.image.effect")) {
                item(.effectGradient)
                item(.effectAlphaGradient)
                item(.effectAirbrush)
                item(.effectBoundaryAirbrush)
                item(.effectBlur)
                item(.effectStamp)
                item(.effectDust)
                item(.effectAlphaView)
            }
            Menu(language.text("menu.image.adjustment")) {
                item(.adjustmentCreate)
                item(.adjustmentEdit)
                item(.adjustmentToggle)
                item(.adjustmentMoveTop)
                item(.adjustmentPrevious)
                item(.adjustmentNext)
            }
        }
    }

    @CommandsBuilder
    private var workspaceWindowMenu: some Commands {
        CommandMenu(language.text("menu.window")) {
            item(.workspaceNewWindow)
            item(.windowToolPalette)
            item(.windowToolOptions)
            item(.windowColorPane)
            item(.windowLayerPalette)
            item(.windowLocator)
            item(.windowSequence)
            item(.sequencePin)
            item(.windowLightTable)
            item(.lightTablePin)
            item(.windowSubpalette)
            item(.subpalettePin)
            item(.windowBatch)
            item(.windowJobProgress)
            item(.locatorPin)
            item(.locatorFixed)
            item(.locatorAutoscroll)
            Divider()
            item(.workspaceReset)
            item(.workspaceSave)
            item(.workspaceSaveAs)
            item(.workspaceRestore)
            item(.workspaceMirror)
            Divider()
            Menu(language.text("menu.workspace.presets")) {
                item(.workspacePresetColoring)
                item(.workspacePresetLineCleanup)
                item(.workspacePresetReference)
                item(.workspacePresetBatch)
                item(.workspacePresetFocus)
            }
        }
    }

    @ViewBuilder
    private func item(_ command: InkpodCommandID) -> some View {
        let state = application.commandState(command, context: resolvedCommandContext)
        let sequence = shortcuts.sequence(for: command)
        let label = sequence?.strokes.count ?? 0 > 1
            ? "\(language.commandLabel(command))  [\(sequence!.description)]"
            : language.commandLabel(command)
        Button {
            _ = application.route(command, context: resolvedCommandContext)
        } label: {
            if state.checked {
                Label(label, systemImage: "checkmark")
            } else {
                Text(label)
            }
        }
        .disabled(!state.enabled)
        .modifier(SingleStrokeShortcut(sequence: sequence))
        .accessibilityIdentifier("inkpod.command.\(command.rawValue)")
    }

    private var resolvedCommandContext: CommandTargetContext? {
        application.commandContext(for: NSApp.keyWindow) ?? context
    }

    @ViewBuilder
    private var historyItems: some View {
        if let projection = application.historyProjection(for: resolvedCommandContext),
           !projection.items.isEmpty
        {
            ForEach(projection.items) { historyItem in
                Button {
                    application.jumpHistory(
                        to: historyItem.index + 1,
                        context: resolvedCommandContext
                    )
                } label: {
                    if historyItem.isApplied {
                        Label(historyLabel(historyItem), systemImage: "checkmark")
                    } else {
                        Text(historyLabel(historyItem))
                    }
                }
            }
        } else {
            Text(language.text("history.empty"))
        }
    }

    private func historyLabel(_ item: CoreHistoryItemProjection) -> String {
        let key: String = switch item.kind {
        case .raster: "history.kind.raster"
        case .palette: "history.kind.palette"
        case .colorChart: "history.kind.colorChart"
        case .mainLineColor: "history.kind.mainLineColor"
        case .document: "history.kind.document"
        }
        return "\(item.index + 1). \(language.text(key))"
    }
}

private struct SingleStrokeShortcut: ViewModifier {
    let sequence: ShortcutSequence?

    func body(content: Content) -> some View {
        if let shortcut = keyboardShortcut {
            content.keyboardShortcut(shortcut.key, modifiers: shortcut.modifiers)
        } else {
            content
        }
    }

    private var keyboardShortcut: (key: KeyEquivalent, modifiers: EventModifiers)? {
        guard let stroke = sequence?.strokes.only,
              case let .unicodeScalar(value) = stroke.key,
              let scalar = UnicodeScalar(value)
        else {
            return nil
        }
        var modifiers: EventModifiers = []
        if stroke.modifiers.contains(.primary) { modifiers.insert(.command) }
        if stroke.modifiers.contains(.shift) { modifiers.insert(.shift) }
        if stroke.modifiers.contains(.alternate) { modifiers.insert(.option) }
        if stroke.modifiers.contains(.control) { modifiers.insert(.control) }
        return (KeyEquivalent(Character(String(scalar))), modifiers)
    }
}

private extension Collection {
    var only: Element? {
        count == 1 ? first : nil
    }
}

public struct InkpodSettingsView: View {
    @ObservedObject private var application: ApplicationCoordinator
    @ObservedObject private var shortcuts: ShortcutController
    @ObservedObject private var language: AppLanguageController
    @State private var searchText = ""

    public init(application: ApplicationCoordinator) {
        self.application = application
        shortcuts = application.shortcutController
        language = application.languageController
    }

    public var body: some View {
        TabView {
            shortcutSettings
                .tabItem { Label(language.text("settings.shortcuts"), systemImage: "keyboard") }
            languageSettings
                .tabItem { Label(language.text("settings.language"), systemImage: "globe") }
        }
        .frame(width: 620, height: 470)
        .padding()
    }

    private var shortcutSettings: some View {
        VStack(alignment: .leading, spacing: 12) {
            TextField(language.text("settings.search"), text: $searchText)
                .textFieldStyle(.roundedBorder)
                .accessibilityIdentifier("inkpod.settings.search")
            List(filteredCommands, id: \.self) { command in
                HStack {
                    Text(language.commandLabel(command))
                    Spacer()
                    Text(shortcuts.sequence(for: command)?.description ?? "—")
                        .foregroundStyle(.secondary)
                        .monospaced()
                    Button(language.text("settings.record")) {
                        shortcuts.beginCapture(for: command)
                    }
                    .accessibilityIdentifier("inkpod.shortcut.record.\(command.rawValue)")
                }
            }
            if let command = shortcuts.captureCommand {
                GroupBox(language.commandLabel(command)) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(language.text("settings.capture.instructions"))
                        Text(shortcuts.capturedStrokes.map(\.description).joined(separator: " "))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .monospaced()
                        if let result = shortcuts.lastEditResult,
                           result != .applied, result != .noOp
                        {
                            Text(language.text("shortcut.result.\(result)"))
                                .foregroundStyle(.red)
                        }
                        HStack {
                            Button(language.text("action.apply")) {
                                _ = shortcuts.commitCapture()
                            }
                            .disabled(shortcuts.capturedStrokes.isEmpty)
                            .accessibilityIdentifier("inkpod.shortcut.apply")
                            Button(language.text("action.cancel"), role: .cancel) {
                                shortcuts.cancelCapture()
                            }
                            .accessibilityIdentifier("inkpod.shortcut.cancel")
                        }
                    }
                }
            }
            HStack {
                Spacer()
                Button(language.commandLabel(.shortcutReset)) {
                    _ = application.route(.shortcutReset, context: nil)
                }
                .accessibilityIdentifier("inkpod.shortcut.reset")
            }
        }
    }

    private var languageSettings: some View {
        Form {
            Picker(language.text("settings.language"), selection: languageBinding) {
                Text(language.commandLabel(.languageSystem)).tag(AppLanguageSelection.system)
                Text(language.commandLabel(.languageJapanese)).tag(AppLanguageSelection.japanese)
                Text(language.commandLabel(.languageEnglish)).tag(AppLanguageSelection.english)
            }
            Text(language.text("settings.language.restart"))
                .foregroundStyle(.secondary)
        }
        .formStyle(.grouped)
    }

    private var languageBinding: Binding<AppLanguageSelection> {
        Binding(
            get: { language.selection },
            set: { value in
                let command: InkpodCommandID = switch value {
                case .system: .languageSystem
                case .japanese: .languageJapanese
                case .english: .languageEnglish
                }
                _ = application.route(command, context: nil)
            }
        )
    }

    private var filteredCommands: [InkpodCommandID] {
        InkpodCommandID.allCases.filter { command in
            command != .shortcutEdit
                && (searchText.isEmpty
                    || language.commandLabel(command).localizedCaseInsensitiveContains(searchText))
        }
    }
}

struct WorkspaceCommandInputSheet: View {
    @ObservedObject var model: WorkspaceModel
    let input: WorkspaceCommandInput
    @ObservedObject var language: AppLanguageController
    @State private var value1: Double
    @State private var value2: Double
    @State private var value3: Double
    @State private var value4: Double
    @State private var value5: Double

    init(
        model: WorkspaceModel,
        input: WorkspaceCommandInput,
        language: AppLanguageController
    ) {
        self.model = model
        self.input = input
        self.language = language
        switch input {
        case let .zoomPercent(percent):
            _value1 = State(initialValue: percent)
            _value2 = State(initialValue: 0)
            _value3 = State(initialValue: 0)
            _value4 = State(initialValue: 0)
            _value5 = State(initialValue: 0)
        case let .boxZoom(x, y, width, height):
            _value1 = State(initialValue: Double(x))
            _value2 = State(initialValue: Double(y))
            _value3 = State(initialValue: Double(width))
            _value4 = State(initialValue: Double(height))
            _value5 = State(initialValue: 0)
        case let .addGuide(_, position), let .moveGuide(_, position):
            _value1 = State(initialValue: Double(position))
            _value2 = State(initialValue: 0)
            _value3 = State(initialValue: 0)
            _value4 = State(initialValue: 0)
            _value5 = State(initialValue: 0)
        case let .grid(grid):
            _value1 = State(initialValue: Double(grid.originX))
            _value2 = State(initialValue: Double(grid.originY))
            _value3 = State(initialValue: Double(grid.spacingX))
            _value4 = State(initialValue: Double(grid.spacingY))
            _value5 = State(initialValue: Double(grid.subdivisions))
        case let .selectionAdjust(_, pixels):
            _value1 = State(initialValue: Double(pixels))
            _value2 = State(initialValue: 0)
            _value3 = State(initialValue: 0)
            _value4 = State(initialValue: 0)
            _value5 = State(initialValue: 0)
        }
    }

    var body: some View {
        Form {
            switch input {
            case .zoomPercent:
                numericField(language.text("input.zoom.percent"), value: $value1)
            case .boxZoom:
                numericField(language.text("input.x"), value: $value1)
                numericField(language.text("input.y"), value: $value2)
                numericField(language.text("input.width"), value: $value3)
                numericField(language.text("input.height"), value: $value4)
            case .addGuide, .moveGuide:
                numericField(language.text("input.position"), value: $value1)
            case .grid:
                numericField(language.text("input.origin.x"), value: $value1)
                numericField(language.text("input.origin.y"), value: $value2)
                numericField(language.text("input.spacing.x"), value: $value3)
                numericField(language.text("input.spacing.y"), value: $value4)
                numericField(language.text("input.subdivisions"), value: $value5)
            case .selectionAdjust:
                numericField(language.text("input.selection.pixels"), value: $value1)
            }
            HStack {
                Spacer()
                Button(language.text("action.cancel"), role: .cancel) {
                    model.cancelCommandInput()
                }
                Button(language.text("action.apply")) { submit() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(minWidth: 360)
    }

    private func numericField(_ title: String, value: Binding<Double>) -> some View {
        TextField(title, value: value, format: .number)
    }

    private func submit() {
        let result: WorkspaceCommandInput? = switch input {
        case .zoomPercent:
            .zoomPercent(value1)
        case .boxZoom:
            integers([value1, value2, value3, value4]).map {
                .boxZoom(x: $0[0], y: $0[1], width: $0[2], height: $0[3])
            }
        case let .addGuide(axis, _):
            integer(value1).map { .addGuide(axis: axis, position: $0) }
        case let .moveGuide(id, _):
            integer(value1).map { .moveGuide(id: id, position: $0) }
        case .grid:
            integers([value1, value2, value3, value4, value5]).flatMap { values in
                guard values[2] > 0, values[3] > 0, values[4] > 0 else { return nil }
                return .grid(CoreGridDefinition(
                    originX: values[0],
                    originY: values[1],
                    spacingX: UInt32(values[2]),
                    spacingY: UInt32(values[3]),
                    subdivisions: UInt32(values[4])
                ))
            }
        case let .selectionAdjust(operation, _):
            unsignedInteger(value1).map { .selectionAdjust(operation, $0) }
        }
        guard let result else { return }
        _ = model.submitCommandInput(result)
    }

    private func integer(_ value: Double) -> Int32? {
        guard value.isFinite, value.rounded() == value,
              value >= Double(Int32.min), value <= Double(Int32.max)
        else {
            return nil
        }
        return Int32(value)
    }

    private func integers(_ values: [Double]) -> [Int32]? {
        let converted = values.compactMap { integer($0) }
        return converted.count == values.count ? converted : nil
    }

    private func unsignedInteger(_ value: Double) -> UInt32? {
        guard value.isFinite, value.rounded() == value,
              (1 ... 4_096).contains(value)
        else {
            return nil
        }
        return UInt32(value)
    }
}
