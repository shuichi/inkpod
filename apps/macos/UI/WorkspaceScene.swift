import SwiftUI

public struct InkpodWorkspaceScene: View {
    @StateObject private var model: WorkspaceModel
    @ObservedObject private var language: AppLanguageController
    @Environment(\.openSettings) private var openSettings
    @Environment(\.openWindow) private var openWindow
    private let application: ApplicationCoordinator

    public init(id: WorkspaceID, application: ApplicationCoordinator) {
        _model = StateObject(wrappedValue: application.workspace(for: id))
        language = application.languageController
        self.application = application
    }

    public var body: some View {
        Group {
            switch model.phase {
            case .idle, .starting:
                ProgressView()
                    .controlSize(.large)
                    .accessibilityIdentifier("inkpod.workspace.loading")
            case .ready:
                WorkspaceEditorView(model: model, language: language)
                    .background(Color(nsColor: .windowBackgroundColor))
                    .contextMenu {
                        commandButton(.zoomIn)
                        commandButton(.zoomOut)
                        commandButton(.fit)
                        commandButton(.oneToOne)
                        Divider()
                        commandButton(.grid)
                    }
            case .failed:
                ContentUnavailableView(
                    language.text("workspace.failure"),
                    systemImage: "exclamationmark.triangle"
                )
                .accessibilityIdentifier("inkpod.workspace.failure")
            case .stopped:
                EmptyView()
            }
        }
        .frame(minWidth: 640, minHeight: 480)
        .focusedSceneValue(\.inkpodCommandTarget, model.commandContext)
        .toolbar {
            ToolbarItem(placement: .navigation) {
                commandButton(.windowToolPalette, icon: "sidebar.left")
            }
        }
        .sheet(item: commandInputBinding) { input in
            WorkspaceCommandInputSheet(model: model, input: input, language: language)
        }
        .sheet(item: newCellBinding) { draft in
            NewCellSheet(model: model, draft: draft, language: language)
        }
        .sheet(item: cellEditorBinding) { draft in
            CellEditorSheet(model: model, draft: draft, language: language)
        }
        .sheet(item: treeEditorBinding) { draft in
            TreeEditorSheet(model: model, draft: draft, language: language)
        }
        .sheet(item: floatingTransformBinding) { draft in
            FloatingTransformSheet(model: model, draft: draft, language: language)
                .interactiveDismissDisabled()
        }
        .sheet(item: m8EditorBinding) { draft in
            M8EditorSheet(model: model, draft: draft, language: language)
                .interactiveDismissDisabled()
        }
        .sheet(item: m9EditorBinding) { draft in
            M9EditorSheet(model: model, draft: draft, language: language)
                .interactiveDismissDisabled()
        }
        .sheet(item: recoveryDecisionBinding) { candidate in
            VStack(alignment: .leading, spacing: 16) {
                Text(language.text("recovery.startup.title"))
                    .font(.headline)
                Text(recoveryDescription(candidate))
                    .foregroundStyle(.secondary)
                HStack {
                    Button(language.text("recovery.action.discard"), role: .destructive) {
                        model.discardPendingRecoveryCandidate()
                    }
                    Spacer()
                    Button(language.text("recovery.action.defer"), role: .cancel) {
                        model.deferPendingRecoveryCandidate()
                    }
                    Button(language.text("recovery.action.restore")) {
                        model.recoverPendingCandidate()
                    }
                    .keyboardShortcut(.defaultAction)
                }
            }
            .padding()
            .frame(width: 440)
            .interactiveDismissDisabled()
        }
        .alert(item: fileOperationAlertBinding) { alert in
            Alert(
                title: Text(language.text(alert.titleKey)),
                message: Text(language.text(alert.messageKey)),
                dismissButton: .default(Text(language.text("action.ok"))) {
                    model.dismissFileOperationAlert()
                }
            )
        }
        .onAppear {
            let action = openSettings
            application.installSettingsPresenter { action() }
        }
        .task {
            application.installWorkspaceOpener { id in openWindow(value: id) }
            application.installBatchWindowOpener { id in openWindow(value: id) }
            model.start(opening: application.startupItem(for: model.id))
        }
        .onDisappear {
            Task { await model.stop() }
        }
    }

    private var commandInputBinding: Binding<WorkspaceCommandInput?> {
        Binding(
            get: { model.pendingCommandInput },
            set: { if $0 == nil { model.cancelCommandInput() } }
        )
    }

    private var newCellBinding: Binding<NewCellDraft?> {
        Binding(
            get: { model.pendingNewCellDraft },
            set: { if $0 == nil { model.cancelNewCell() } }
        )
    }

    private var cellEditorBinding: Binding<CellEditorDraft?> {
        Binding(
            get: { model.pendingCellEditor },
            set: { if $0 == nil { model.cancelM5Editor() } }
        )
    }

    private var treeEditorBinding: Binding<TreeEditorDraft?> {
        Binding(
            get: { model.pendingTreeEditor },
            set: { if $0 == nil { model.cancelM5Editor() } }
        )
    }

    private var floatingTransformBinding: Binding<FloatingTransformDraft?> {
        Binding(
            get: { model.floatingTransformEditor },
            set: { if $0 == nil { model.cancelPendingPaste() } }
        )
    }

    private var m8EditorBinding: Binding<M8EditorDraft?> {
        Binding(
            get: { model.pendingM8Editor },
            set: { if $0 == nil { model.cancelM8Editor() } }
        )
    }

    private var m9EditorBinding: Binding<M9EditorDraft?> {
        Binding(
            get: { model.pendingM9Editor },
            set: { if $0 == nil { model.cancelM9Editor() } }
        )
    }

    private var recoveryDecisionBinding: Binding<RecoveryCandidate?> {
        Binding(
            get: { model.pendingRecoveryDecision },
            set: { if $0 == nil { model.deferPendingRecoveryCandidate() } }
        )
    }

    private var fileOperationAlertBinding: Binding<WorkspaceFileOperationAlert?> {
        Binding(
            get: { model.fileOperationAlert },
            set: { if $0 == nil { model.dismissFileOperationAlert() } }
        )
    }

    private func recoveryDescription(_ candidate: RecoveryCandidate) -> String {
        let key = candidate.metadataIsValid
            ? "recovery.startup.body" : "recovery.startup.body.malformed"
        let source = candidate.originalPath.map {
            URL(filePath: $0).lastPathComponent
        } ?? candidate.artifactURL.lastPathComponent
        return language.text(key).replacingOccurrences(of: "%@", with: source)
    }

    @ViewBuilder
    private func commandButton(_ command: InkpodCommandID, icon: String? = nil) -> some View {
        let state = model.commandContext.map { model.commandState(command, context: $0) }
            ?? CommandState(enabled: false)
        Button {
            guard let context = model.commandContext else { return }
            _ = model.execute(command, context: context)
        } label: {
            if let icon {
                Label(language.commandLabel(command), systemImage: icon)
            } else if state.checked {
                Label(language.commandLabel(command), systemImage: "checkmark")
            } else {
                Text(language.commandLabel(command))
            }
        }
        .disabled(!state.enabled)
        .accessibilityIdentifier("inkpod.command.\(command.rawValue)")
    }
}

private struct M8EditorSheet: View {
    @ObservedObject var model: WorkspaceModel
    let draft: M8EditorDraft
    @ObservedObject var language: AppLanguageController

    var body: some View {
        switch draft {
        case let .filter(value):
            M8FilterSheet(model: model, draft: value, language: language)
        case let .effect(value):
            M8EffectSheet(model: model, draft: value, language: language)
        case let .geometry(value):
            M8GeometrySheet(model: model, draft: value, language: language)
        case let .annotation(value):
            M8AnnotationSheet(model: model, draft: value, language: language)
        case let .shootingFrame(value):
            M8ShootingFrameSheet(model: model, draft: value, language: language)
        case let .vanishingPoint(value):
            M8VanishingPointSheet(model: model, draft: value, language: language)
        }
    }
}

private struct M8FilterSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: M8FilterDraft

    init(model: WorkspaceModel, draft: M8FilterDraft, language: AppLanguageController) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Text(language.commandLabel(commandID)).font(.headline)
            Picker(language.text("m8.filter.channel"), selection: $draft.channel) {
                ForEach(CoreFilterChannel.allCases, id: \.rawValue) { channel in
                    Text(String(describing: channel)).tag(channel)
                }
            }
            ForEach(draft.parameters.indices, id: \.self) { index in
                LabeledContent("P\(index + 1)") {
                    TextField("", value: $draft.parameters[index], format: .number)
                        .frame(width: 120)
                }
            }
            if draft.kind == .toneCurve {
                Picker(language.text("m8.filter.interpolation"), selection: $draft.interpolation) {
                    Text("Bézier").tag(CoreCurveInterpolation.bezier)
                    Text("B-spline").tag(CoreCurveInterpolation.bSpline)
                }
            }
            HStack {
                Button(language.text("action.cancel"), role: .cancel) {
                    model.cancelM8Editor()
                }
                Spacer()
                Button(language.text("action.apply")) {
                    model.applyFilterEditor(draft)
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 420)
        .onChange(of: draft) { _, value in model.updateFilterPreview(value) }
        .accessibilityIdentifier("inkpod.m8.filter-sheet")
    }

    private var commandID: InkpodCommandID {
        switch draft.kind {
        case .invert: .filterInvert
        case .blurWeak: .filterBlurWeak
        case .sharpenWeak: .filterSharpenWeak
        case .sharpenStrong: .filterSharpenStrong
        case .blurStrong: .filterBlurStrong
        case .gaussianBlur: .filterGaussian
        case .autoContrast: .filterAutoContrast
        case .brightnessContrast: .filterBrightness
        case .toneCurve: .filterToneCurve
        case .levels: .filterLevels
        case .hsv: .filterHSV
        case .colorBalance: .filterColorBalance
        case .unsharpMask: .filterUnsharp
        }
    }
}

private struct M8EffectSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: M8EffectDraft

    init(model: WorkspaceModel, draft: M8EffectDraft, language: AppLanguageController) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Text(language.commandLabel(draft.command)).font(.headline)
            LabeledContent(language.text("m8.effect.primary")) {
                TextField("", value: $draft.primary, format: .number).frame(width: 120)
            }
            LabeledContent(language.text("m8.effect.strength")) {
                TextField("", value: $draft.secondary, format: .number).frame(width: 120)
            }
            if draft.command == .effectDust {
                LabeledContent(language.text("m8.effect.maximumPixels")) {
                    TextField("", value: $draft.maximumPixels, format: .number)
                        .frame(width: 120)
                }
            }
            M8SheetActions(language: language, cancel: model.cancelM8Editor) {
                model.applyEffectEditor(draft)
            }
        }
        .padding().frame(width: 420)
        .accessibilityIdentifier("inkpod.m8.effect-sheet")
    }
}

private struct M8GeometrySheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: M8GeometryOptionsDraft

    init(model: WorkspaceModel, draft: M8GeometryOptionsDraft, language: AppLanguageController) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Text(language.text("m8.geometry.title")).font(.headline)
            Toggle(language.text("m8.geometry.outline"), isOn: $draft.options.outline)
            Toggle(language.text("m8.geometry.fill"), isOn: $draft.options.fill)
            Toggle(language.text("m8.geometry.close"), isOn: $draft.options.closePath)
            Toggle(language.text("m8.geometry.constrain"), isOn: $draft.options.constrainTo45Degrees)
            Toggle(language.text("m8.geometry.center"), isOn: $draft.options.fromCenter)
            LabeledContent(language.text("m8.geometry.width")) {
                TextField("", value: $draft.outlineWidth, format: .number).frame(width: 120)
            }
            LabeledContent(language.text("m8.geometry.sides")) {
                TextField("", value: $draft.polygonSides, format: .number).frame(width: 120)
            }
            M8SheetActions(language: language, cancel: model.cancelM8Editor) {
                model.applyGeometryOptions(draft)
            }
        }
        .padding().frame(width: 420)
        .accessibilityIdentifier("inkpod.m8.geometry-sheet")
    }
}

private struct M8AnnotationSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: M8AnnotationDraft

    init(model: WorkspaceModel, draft: M8AnnotationDraft, language: AppLanguageController) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Text(language.text("m8.annotation.title")).font(.headline)
            TextEditor(text: $draft.text).frame(minHeight: 90)
            TextField(language.text("m8.annotation.font"), text: $draft.fontFamily)
            LabeledContent(language.text("m8.annotation.fontSize")) {
                TextField("", value: $draft.fontSize, format: .number).frame(width: 120)
            }
            HStack {
                TextField("X", value: $draft.x, format: .number)
                TextField("Y", value: $draft.y, format: .number)
                TextField("W", value: $draft.width, format: .number)
                TextField("H", value: $draft.height, format: .number)
            }
            Toggle(language.text("m8.annotation.instruction"), isOn: $draft.instructionOnly)
            M8SheetActions(language: language, cancel: model.cancelM8Editor) {
                model.applyAnnotationEditor(draft)
            }
        }
        .padding().frame(width: 480)
        .accessibilityIdentifier("inkpod.m8.annotation-sheet")
    }
}

private struct M8ShootingFrameSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: M8ShootingFrameDraft

    init(model: WorkspaceModel, draft: M8ShootingFrameDraft, language: AppLanguageController) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Text(language.text("m8.frame.title")).font(.headline)
            M8DoubleField(label: "X", value: $draft.frame.centerX)
            M8DoubleField(label: "Y", value: $draft.frame.centerY)
            M8DoubleField(label: "W", value: $draft.frame.width)
            M8DoubleField(label: "H", value: $draft.frame.height)
            M8DoubleField(label: language.text("m8.frame.rotation"), value: $draft.frame.rotationDegrees)
            Toggle(language.text("m8.frame.visible"), isOn: $draft.frame.visible)
            Toggle(language.text("m8.frame.export"), isOn: $draft.frame.includeInInstructionExport)
            M8SheetActions(language: language, cancel: model.cancelM8Editor) {
                model.applyShootingFrame(draft)
            }
        }
        .padding().frame(width: 420)
        .onChange(of: draft) { _, value in model.previewShootingFrame(value) }
        .accessibilityIdentifier("inkpod.m8.frame-sheet")
    }
}

private struct M8VanishingPointSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: M8VanishingPointDraft

    init(model: WorkspaceModel, draft: M8VanishingPointDraft, language: AppLanguageController) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Text(language.text("m8.vp.title")).font(.headline)
            LabeledContent("X × 1000") {
                TextField("", value: $draft.point.xMilli, format: .number).frame(width: 140)
            }
            LabeledContent("Y × 1000") {
                TextField("", value: $draft.point.yMilli, format: .number).frame(width: 140)
            }
            LabeledContent(language.text("m8.vp.interval")) {
                TextField("", value: $draft.point.intervalMilliDegrees, format: .number)
                    .frame(width: 140)
            }
            LabeledContent(language.text("m8.vp.opacity")) {
                TextField("", value: $draft.point.opacityMilli, format: .number).frame(width: 140)
            }
            Toggle(language.text("m8.frame.visible"), isOn: $draft.point.visible)
            M8SheetActions(language: language, cancel: model.cancelM8Editor) {
                model.applyVanishingPoint(draft)
            }
        }
        .padding().frame(width: 420)
        .onChange(of: draft) { _, value in model.previewVanishingPoint(value) }
        .accessibilityIdentifier("inkpod.m8.vp-sheet")
    }
}

private struct M8SheetActions: View {
    @ObservedObject var language: AppLanguageController
    let cancel: () -> Void
    let apply: () -> Void

    var body: some View {
        HStack {
            Button(language.text("action.cancel"), role: .cancel, action: cancel)
            Spacer()
            Button(language.text("action.apply"), action: apply).keyboardShortcut(.defaultAction)
        }
    }
}

private struct M8DoubleField: View {
    let label: String
    @Binding var value: Double

    var body: some View {
        LabeledContent(label) { TextField("", value: $value, format: .number).frame(width: 140) }
    }
}

private struct FloatingTransformSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: FloatingTransformDraft

    init(
        model: WorkspaceModel,
        draft: FloatingTransformDraft,
        language: AppLanguageController
    ) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Picker(language.text("transform.anchor"), selection: $draft.anchor) {
                Text(language.text("transform.anchor.topLeft")).tag(CoreFloatingAnchor.topLeft)
                Text(language.text("transform.anchor.topRight")).tag(CoreFloatingAnchor.topRight)
                Text(language.text("transform.anchor.center")).tag(CoreFloatingAnchor.center)
                Text(language.text("transform.anchor.bottomLeft"))
                    .tag(CoreFloatingAnchor.bottomLeft)
                Text(language.text("transform.anchor.bottomRight"))
                    .tag(CoreFloatingAnchor.bottomRight)
            }
            LabeledContent(language.text("input.x")) {
                TextField("", value: $draft.targetX, format: .number)
            }
            LabeledContent(language.text("input.y")) {
                TextField("", value: $draft.targetY, format: .number)
            }
            LabeledContent(language.text("transform.scale.x")) {
                TextField("", value: $draft.scaleX, format: .number)
            }
            LabeledContent(language.text("transform.scale.y")) {
                TextField("", value: $draft.scaleY, format: .number)
            }
            LabeledContent(language.text("transform.rotation")) {
                TextField("", value: $draft.rotationDegrees, format: .number)
            }
            HStack {
                Spacer()
                Button(language.text("action.cancel"), role: .cancel) {
                    model.cancelPendingPaste()
                }
                Button(language.text("action.apply")) {
                    model.commitPendingTransform(draft)
                }
                .keyboardShortcut(.defaultAction)
                .disabled(!draft.transform.isValid)
            }
        }
        .padding()
        .frame(width: 420)
        .onChange(of: draft) { _, replacement in
            model.previewPendingPaste(replacement)
        }
        .accessibilityIdentifier("inkpod.floating.transform.sheet")
    }
}
