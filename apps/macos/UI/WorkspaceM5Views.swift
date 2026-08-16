import SwiftUI

struct WorkspaceEditorView: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    var body: some View {
        WorkspaceChromeView(model: model, language: language)
    }
}

struct WorkspaceEditorContent: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var splitDragStartRatio: Double?

    var body: some View {
        VStack(spacing: 0) {
            editorGroups
            if model.sequenceVisible {
                Divider()
                M9SequenceTimeline(model: model, language: language)
            }
        }
    }

    @ViewBuilder
    private var editorGroups: some View {
        if let graph = model.editorGraph {
            if graph.groups.count == 2, let orientation = graph.splitOrientation {
                ratioSplit(
                    first: graph.groups[0],
                    second: graph.groups[1],
                    orientation: orientation
                )
            } else if let group = graph.groups.first {
                editorGroup(group)
            }
        }
    }

    private func ratioSplit(
        first: EditorGroupRecord,
        second: EditorGroupRecord,
        orientation: WorkspaceSplitOrientation
    ) -> some View {
        GeometryReader { geometry in
            let dividerLength: CGFloat = 8
            let available = max(
                orientation == .horizontal ? geometry.size.width : geometry.size.height,
                dividerLength
            ) - dividerLength
            let firstLength = available * CGFloat(model.workspaceLayout.splitRatio)
            if orientation == .horizontal {
                HStack(spacing: 0) {
                    editorGroup(first).frame(width: firstLength)
                    splitDivider(orientation: orientation, totalLength: available)
                        .frame(width: dividerLength)
                    editorGroup(second).frame(maxWidth: .infinity)
                }
            } else {
                VStack(spacing: 0) {
                    editorGroup(first).frame(height: firstLength)
                    splitDivider(orientation: orientation, totalLength: available)
                        .frame(height: dividerLength)
                    editorGroup(second).frame(maxHeight: .infinity)
                }
            }
        }
    }

    private func splitDivider(
        orientation: WorkspaceSplitOrientation,
        totalLength: CGFloat
    ) -> some View {
        Rectangle()
            .fill(Color.clear)
            .overlay(Divider())
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        let start = splitDragStartRatio ?? model.workspaceLayout.splitRatio
                        splitDragStartRatio = start
                        let translation = orientation == .horizontal
                            ? value.translation.width : value.translation.height
                        model.updateSplitRatio(
                            start + Double(translation / max(totalLength, 1))
                        )
                    }
                    .onEnded { _ in splitDragStartRatio = nil }
            )
            .accessibilityLabel(language.text("m5.workspace.splitDivider"))
            .accessibilityValue(
                "\(Int((model.workspaceLayout.splitRatio * 100).rounded()))%"
            )
    }

    private func editorGroup(_ group: EditorGroupRecord) -> some View {
        VStack(spacing: 0) {
            ScrollView(.horizontal) {
                HStack(spacing: 2) {
                    ForEach(group.views) { view in
                        Button {
                            model.activate(groupID: group.id, viewID: view.id)
                        } label: {
                            HStack(spacing: 6) {
                                if group.activeViewID == view.id {
                                    Image(systemName: "circle.fill")
                                        .font(.system(size: 6))
                                }
                                Text(view.title)
                                    .lineLimit(1)
                            }
                            .padding(.horizontal, 8)
                            .padding(.vertical, 5)
                        }
                        .buttonStyle(.borderless)
                        .background(
                            group.activeViewID == view.id
                                ? Color.accentColor.opacity(0.18) : Color.clear,
                            in: RoundedRectangle(cornerRadius: 5)
                        )
                        .draggable(String(view.id.rawValue))
                        .accessibilityIdentifier("inkpod.tab.\(view.id.rawValue)")
                    }
                }
                .padding(.horizontal, 5)
            }
            .frame(height: 34)
            .background(.bar)
            .dropDestination(for: String.self) { values, _ in
                guard let raw = values.first.flatMap(UInt64.init) else { return false }
                return model.moveView(WorkspaceViewID(rawValue: raw), to: group.id)
            }
            Divider()
            if let active = group.activeView {
                CanvasSurfaceView(model: model, viewID: active.id)
                    .id(active.id)
                    .background(Color(nsColor: .windowBackgroundColor))
            }
        }
        .frame(minWidth: 280, minHeight: 240)
    }
}

struct M8VectorAnnotationInspector: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(language.text("m8.inspector.title")).font(.headline)
            GroupBox(language.text("m8.inspector.filter")) {
                HStack {
                    commandButton(.filterInvert, icon: "circle.lefthalf.filled.inverse")
                    commandButton(.filterBrightness, icon: "sun.max")
                }
                .buttonStyle(.borderless)
            }
            GroupBox(language.text("m8.inspector.vector")) {
                HStack {
                    commandButton(.vectorLine, icon: "line.diagonal")
                    commandButton(.vectorCurve, icon: "point.topleft.down.curvedto.point.bottomright.up")
                    commandButton(.vectorEraser, icon: "eraser")
                    commandButton(.geometryOptions, icon: "slider.horizontal.3")
                }
                .buttonStyle(.borderless)
            }
            GroupBox(language.text("m8.inspector.annotation")) {
                HStack {
                    commandButton(.annotationAddText, icon: "textformat")
                    commandButton(.annotationDrawInstruction, icon: "pencil.tip")
                    commandButton(.annotationSelectPrevious, icon: "chevron.left")
                    commandButton(.annotationSelectNext, icon: "chevron.right")
                }
                .buttonStyle(.borderless)
            }
            GroupBox(language.text("m8.inspector.guides")) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(model.m8State?.shootingFrame == nil
                        ? language.text("m8.frame.none") : language.text("m8.frame.present"))
                    Text(language.text("m8.vp.count") + " \(model.m8State?.vanishingPoints.count ?? 0)")
                    HStack {
                        commandButton(.cellShootingFrameProperties, icon: "rectangle.dashed")
                        commandButton(.cellVanishingPointProperties, icon: "scope")
                    }
                    .buttonStyle(.borderless)
                }
            }
            Spacer()
        }
        .padding(8)
        .frame(minWidth: 210, idealWidth: 230, maxWidth: 280)
        .accessibilityIdentifier("inkpod.m8.inspector")
    }

    @ViewBuilder
    private func commandButton(_ command: InkpodCommandID, icon: String) -> some View {
        let state = model.commandContext.map { model.commandState(command, context: $0) }
            ?? CommandState(enabled: false)
        Button {
            guard let context = model.commandContext else { return }
            _ = model.execute(command, context: context)
        } label: {
            Image(systemName: icon)
        }
        .help(language.commandLabel(command))
        .disabled(!state.enabled)
        .accessibilityLabel(language.commandLabel(command))
        .accessibilityIdentifier("inkpod.command.\(command.rawValue)")
    }
}

struct M7HistoryInspector: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.text("history.inspector.title"))
                    .font(.headline)
                Spacer()
                Button {
                    model.refreshHistory(rebuildVisualization: true)
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(language.text("history.inspector.refresh"))
            }
            .padding(8)
            if let progress = model.historyProgress, !progress.isComplete {
                HStack {
                    ProgressView(
                        value: Double(progress.completedEvents),
                        total: Double(max(progress.totalEvents, 1))
                    )
                    .accessibilityLabel(language.text("history.inspector.progress"))
                    Button(language.text("action.cancel"), role: .cancel) {
                        model.cancelHistoryVisualization()
                    }
                }
                .padding(.horizontal, 8)
            }
            if model.historyRows.isEmpty {
                ContentUnavailableView(
                    language.text("history.empty"),
                    systemImage: "clock.arrow.circlepath"
                )
            } else {
                List(model.historyRows) { row in
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(row.primitiveName)
                                .lineLimit(1)
                            Spacer()
                            Text("B\(row.branchID)")
                                .font(.caption.monospacedDigit())
                                .foregroundStyle(.secondary)
                        }
                        if !row.arguments.isEmpty {
                            Text(row.arguments)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        }
                        Text("S\(row.committedStateID) · #\(row.journalEventID)")
                            .font(.caption2.monospacedDigit())
                            .foregroundStyle(.tertiary)
                    }
                    .onAppear { model.loadMoreHistoryRowsIfNeeded(after: row) }
                }
            }
        }
        .frame(minWidth: 240, idealWidth: 280, maxWidth: 380)
        .accessibilityIdentifier("inkpod.m7.history-inspector")
    }
}

struct LayerPlaneInspector: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text(language.text("m5.inspector.layers"))
                    .font(.headline)
                Spacer()
                Button { model.toggleLayerPanePin() } label: {
                    Image(systemName: model.layerPaneIsPinned ? "pin.fill" : "pin")
                }
                .help(language.text(
                    model.layerPaneIsPinned ? "m5.inspector.follow" : "m5.inspector.pin"
                ))
                Button { model.performLayerPane(.layerNew) } label: { Image(systemName: "plus") }
                Button { model.performLayerPane(.layerDuplicate) } label: {
                    Image(systemName: "plus.square.on.square")
                }
                Button(role: .destructive) { model.performLayerPane(.layerDelete) } label: {
                    Image(systemName: "trash")
                }
            }
            .buttonStyle(.borderless)
            .padding(8)
            List(model.cellTree?.layers ?? []) { layer in
                Button {
                    let plane = layer.planes.first?.id ?? 0
                    if plane != 0 { model.selectNode(layerID: layer.id, planeID: plane) }
                } label: {
                    HStack {
                        Image(systemName: layer.isVisible ? "eye" : "eye.slash")
                        Image(systemName: layer.isEditable ? "pencil" : "lock")
                        Text(layer.name)
                        Spacer()
                        Text("\(layer.opacityMilli / 10)%")
                            .foregroundStyle(.secondary)
                    }
                }
                .buttonStyle(.plain)
                .listRowBackground(
                    model.cellTree?.activeLayerID == layer.id
                        ? Color.accentColor.opacity(0.16) : Color.clear
                )
                .draggable("layer:\(layer.id)")
            }
            .accessibilityIdentifier("inkpod.inspector.layers")
            if model.layerPaneAccessibilityNotice == .pinnedDocumentClosed {
                Text(language.text("m5.inspector.pin.closed"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 8)
                    .accessibilityAddTraits(.isStaticText)
            }
            Divider()
            HStack {
                Text(language.text("m5.inspector.planes"))
                    .font(.headline)
                Spacer()
                Button { model.performLayerPane(.planeNew) } label: { Image(systemName: "plus") }
                Button { model.performLayerPane(.planeDuplicate) } label: {
                    Image(systemName: "plus.square.on.square")
                }
                Button(role: .destructive) { model.performLayerPane(.planeDelete) } label: {
                    Image(systemName: "trash")
                }
            }
            .buttonStyle(.borderless)
            .padding(8)
            List(activePlanes) { plane in
                Button {
                    model.selectNode(layerID: plane.parentID, planeID: plane.id)
                } label: {
                    HStack {
                        Image(systemName: plane.isVisible ? "eye" : "eye.slash")
                        Image(systemName: plane.isEditable ? "scope" : "lock")
                        Text(plane.name)
                        Spacer()
                        Text("\(plane.opacityMilli / 10)%")
                            .foregroundStyle(.secondary)
                    }
                }
                .buttonStyle(.plain)
                .listRowBackground(
                    model.cellTree?.activePlaneID == plane.id
                        ? Color.accentColor.opacity(0.16) : Color.clear
                )
                .draggable("plane:\(plane.id)")
            }
            .accessibilityIdentifier("inkpod.inspector.planes")
        }
        .frame(minWidth: 240, idealWidth: model.workspaceLayout.inspectorWidth, maxWidth: 640)
    }

    private var activePlanes: [CoreNodeProjection] {
        guard let tree = model.cellTree else { return [] }
        return tree.layers.first { $0.id == tree.activeLayerID }?.planes ?? []
    }
}

struct NewCellSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: NewCellDraft

    init(
        model: WorkspaceModel,
        draft: NewCellDraft,
        language: AppLanguageController
    ) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            Picker(language.text("m5.field.sizing"), selection: $draft.sizingMode) {
                Text(language.text("m5.sizing.imagePixels"))
                    .tag(CoreCellSizingMode.imagePixels)
                Text(language.text("m5.sizing.frameSize"))
                    .tag(CoreCellSizingMode.frameMicrometres)
            }
            valueField("m5.field.width", value: $draft.width)
            valueField("m5.field.height", value: $draft.height)
            valueField("m5.field.dpiX", value: $draft.dpiXMilli)
            valueField("m5.field.dpiY", value: $draft.dpiYMilli)
            valueField("m5.field.margin", value: $draft.marginMilli)
            valueField("m5.field.safeFrameRatio", value: $draft.safeFrameRatioMilli)
            valueField("m5.field.maximumCloseRatio", value: $draft.maximumCloseRatioMilli)
            valueField("m5.field.cellCount", value: $draft.count)
            Picker(language.text("m5.field.anchor"), selection: $draft.anchor) {
                ForEach(CoreFrameAnchor.allCases, id: \.self) { Text(anchorName($0)).tag($0) }
            }
            Picker(language.text("m5.field.initialLayer"), selection: $draft.initialLayerKind) {
                ForEach(CoreLayerKind.allCases, id: \.self) {
                    Text(layerKindName($0)).tag($0)
                }
            }
            Picker(language.text("m5.field.pixelFormat"), selection: $draft.pixelFormat) {
                ForEach(CorePixelStorageFormat.allCases.filter { $0 != .none }, id: \.self) {
                    Text(pixelFormatName($0)).tag($0)
                }
            }
            if let plan = model.pendingNewCellPlan {
                Section(language.text("m5.new.corePlan")) {
                    LabeledContent(
                        language.text("m5.field.cellCount"),
                        value: "\(plan.items.count)"
                    )
                    LabeledContent(
                        language.text("m5.new.imageSize"),
                        value: "\(plan.items[0].width) × \(plan.items[0].height) px"
                    )
                    LabeledContent(
                        language.text("m5.frame.drawing"),
                        value: rectDescription(plan.items[0].drawingFrame)
                    )
                    LabeledContent(
                        language.text("m5.frame.safe"),
                        value: rectDescription(plan.items[0].safeFrame)
                    )
                }
            }
            HStack {
                Spacer()
                Button(language.text("action.cancel"), role: .cancel) { model.cancelNewCell() }
                    .accessibilityIdentifier("inkpod.newCell.cancel")
                if model.pendingNewCellPlan == nil {
                    Button(language.text("m5.action.preview")) { model.prepareNewCell(draft) }
                        .keyboardShortcut(.defaultAction)
                        .accessibilityIdentifier("inkpod.newCell.preview")
                } else {
                    Button(language.text("m5.action.create")) { model.commitNewCellPlan() }
                        .keyboardShortcut(.defaultAction)
                        .accessibilityIdentifier("inkpod.newCell.create")
                }
            }
        }
        .padding()
        .frame(width: 520)
        .accessibilityIdentifier("inkpod.newCell.sheet")
        .interactiveDismissDisabled(model.pendingNewCellPlan != nil)
    }

    private func valueField(_ key: String, value: Binding<UInt32>) -> some View {
        TextField(language.text(key), value: value, format: .number)
    }

    private func anchorName(_ anchor: CoreFrameAnchor) -> String {
        switch anchor {
        case .topLeft: language.text("m5.anchor.topLeft")
        case .topRight: language.text("m5.anchor.topRight")
        case .center: language.text("m5.anchor.center")
        case .bottomLeft: language.text("m5.anchor.bottomLeft")
        case .bottomRight: language.text("m5.anchor.bottomRight")
        }
    }

    private func layerKindName(_ kind: CoreLayerKind) -> String {
        language.text("m5.layerKind.\(kind.rawValue)")
    }

    private func pixelFormatName(_ format: CorePixelStorageFormat) -> String {
        language.text("m5.pixelFormat.\(format.rawValue)")
    }

    private func rectDescription(_ rect: CoreFrameRect) -> String {
        "\(rect.x), \(rect.y), \(rect.width) × \(rect.height)"
    }
}

struct CellEditorSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: CellEditorDraft

    init(
        model: WorkspaceModel,
        draft: CellEditorDraft,
        language: AppLanguageController
    ) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            switch draft.kind {
            case .paperFrames:
                frameFields("m5.frame.hundred", frame: $draft.frames.hundred)
                frameFields("m5.frame.reference", frame: $draft.frames.reference)
                frameFields("m5.frame.drawing", frame: $draft.frames.drawing)
                frameFields("m5.frame.safe", frame: $draft.frames.safe)
                frameFields("m5.frame.shooting", frame: $draft.frames.shooting)
                frameFields("m5.frame.maximumClose", frame: $draft.frames.maximumClose)
                Section(language.text("m5.field.margins")) {
                    valueField("m5.field.left", value: $draft.frames.margins.left)
                    valueField("m5.field.top", value: $draft.frames.margins.top)
                    valueField("m5.field.right", value: $draft.frames.margins.right)
                    valueField("m5.field.bottom", value: $draft.frames.margins.bottom)
                }
            case let .resize(allowsResample):
                valueField("m5.field.width", value: $draft.width)
                valueField("m5.field.height", value: $draft.height)
                valueField("m5.field.dpiX", value: $draft.dpiXMilli)
                valueField("m5.field.dpiY", value: $draft.dpiYMilli)
                Picker(language.text("m5.field.anchor"), selection: $draft.anchor) {
                    ForEach(CoreFrameAnchor.allCases, id: \.self) {
                        Text(anchorName($0)).tag($0)
                    }
                }
                if allowsResample {
                    Toggle(language.text("m5.field.resample"), isOn: $draft.resample)
                }
            }
            HStack {
                Spacer()
                Button(language.text("action.cancel"), role: .cancel) { model.cancelM5Editor() }
                Button(language.text("action.apply")) { model.submitCellEditor(draft) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 520)
    }

    private func frameFields(_ key: String, frame: Binding<CoreFrameRect>) -> some View {
        Section(language.text(key)) {
            TextField(language.text("m5.field.x"), value: frame.x, format: .number)
            TextField(language.text("m5.field.y"), value: frame.y, format: .number)
            TextField(language.text("m5.field.width"), value: frame.width, format: .number)
            TextField(language.text("m5.field.height"), value: frame.height, format: .number)
        }
    }

    private func valueField(_ key: String, value: Binding<UInt32>) -> some View {
        TextField(language.text(key), value: value, format: .number)
    }

    private func anchorName(_ anchor: CoreFrameAnchor) -> String {
        switch anchor {
        case .topLeft: language.text("m5.anchor.topLeft")
        case .topRight: language.text("m5.anchor.topRight")
        case .center: language.text("m5.anchor.center")
        case .bottomLeft: language.text("m5.anchor.bottomLeft")
        case .bottomRight: language.text("m5.anchor.bottomRight")
        }
    }
}

struct TreeEditorSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: TreeEditorDraft

    init(
        model: WorkspaceModel,
        draft: TreeEditorDraft,
        language: AppLanguageController
    ) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            if usesName {
                TextField(language.text("m5.field.name"), text: $draft.name)
            }
            if usesLayerKind {
                Picker(language.text("m5.field.layerType"), selection: $draft.layerKind) {
                    ForEach(CoreLayerKind.allCases, id: \.self) {
                        Text(language.text("m5.layerKind.\($0.rawValue)")).tag($0)
                    }
                }
            }
            if usesPlaneKind {
                Picker(language.text("m5.field.planeType"), selection: $draft.planeKind) {
                    ForEach(CorePlaneKind.allCases, id: \.self) {
                        Text(language.text("m5.planeKind.\($0.rawValue)")).tag($0)
                    }
                }
            }
            if usesPixelFormat {
                Picker(language.text("m5.field.pixelFormat"), selection: $draft.pixelFormat) {
                    ForEach(CorePixelStorageFormat.allCases.filter { $0 != .none }, id: \.self) {
                        Text(language.text("m5.pixelFormat.\($0.rawValue)")).tag($0)
                    }
                }
            }
            if usesProperties {
                Toggle(language.text("m5.field.visible"), isOn: $draft.visible)
                Toggle(language.text("m5.field.editable"), isOn: $draft.editable)
                TextField(
                    language.text("m5.field.opacity"),
                    value: $draft.opacityMilli,
                    format: .number
                )
            }
            HStack {
                Spacer()
                Button(language.text("action.cancel"), role: .cancel) { model.cancelM5Editor() }
                Button(language.text("action.apply")) { model.submitTreeEditor(draft) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 420)
    }

    private var usesName: Bool {
        switch draft.kind {
        case .convertLayer, .convertPlane: false
        default: true
        }
    }

    private var usesLayerKind: Bool {
        switch draft.kind {
        case .createLayer, .convertLayer: true
        default: false
        }
    }

    private var usesPlaneKind: Bool {
        switch draft.kind {
        case .createPlane, .convertPlane: true
        default: false
        }
    }

    private var usesPixelFormat: Bool {
        switch draft.kind {
        case .createLayer, .createPlane, .convertLayer, .convertPlane: true
        default: false
        }
    }

    private var usesProperties: Bool {
        switch draft.kind {
        case .layerProperties, .planeProperties: true
        default: false
        }
    }
}
