import AppKit
import CoreGraphics
import SwiftUI

struct M9SequenceTimeline: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(language.text("m9.sequence.title")).font(.headline)
                Spacer()
                commandButton(.sequencePrevious, symbol: "chevron.left")
                commandButton(.sequenceNext, symbol: "chevron.right")
                commandButton(.motionStart, symbol: "play.fill")
                commandButton(.motionPause, symbol: "pause.fill")
                commandButton(.motionStop, symbol: "stop.fill")
                commandButton(.sequencePin, symbol: model.sequencePaneTarget.isPinned
                    ? "pin.fill" : "pin")
            }
            .buttonStyle(.borderless)
            if let cut = model.cut {
                ScrollView(.horizontal) {
                    HStack(spacing: 5) {
                        ForEach(Array(cut.members.enumerated()), id: \.offset) { index, member in
                            Button {
                                model.selectCutMember(index)
                            } label: {
                                VStack(alignment: .leading) {
                                    Text("#\(member.displayNumber)")
                                        .font(.caption.monospacedDigit())
                                    Text(member.relativePath).lineLimit(1)
                                }
                                .padding(6)
                                .background(
                                    model.selectedCutMemberIndex == index
                                        ? Color.accentColor.opacity(0.22) : Color.clear,
                                    in: RoundedRectangle(cornerRadius: 5)
                                )
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
                .accessibilityIdentifier("inkpod.m9.cut-timeline")
            }
            ScrollView(.horizontal) {
                HStack(spacing: 8) {
                    ForEach(model.sequenceAnimation?.sequence ?? [], id: \.index) { cell in
                        Button {
                            model.activateSequenceCell(cell.index)
                        } label: {
                            VStack(spacing: 3) {
                                M9Thumbnail(
                                    width: cell.thumbnailWidth,
                                    height: cell.thumbnailHeight,
                                    rgba8: cell.thumbnailRGBA8
                                )
                                .frame(width: 72, height: 54)
                                Text("#\(cell.cellNumber) \(cell.name)")
                                    .font(.caption)
                                    .lineLimit(1)
                            }
                            .padding(5)
                            .background(
                                model.sequenceAnimation?.activeSequenceIndex == cell.index
                                    ? Color.accentColor.opacity(0.22) : Color.clear,
                                in: RoundedRectangle(cornerRadius: 6)
                            )
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("inkpod.m9.sequence.cell.\(cell.index)")
                    }
                }
            }
        }
        .padding(8)
        .frame(minHeight: 92, idealHeight: 132, maxHeight: 180)
        .background(Color(nsColor: .controlBackgroundColor))
        .accessibilityIdentifier("inkpod.m9.sequence-timeline")
    }

    private func commandButton(_ command: InkpodCommandID, symbol: String) -> some View {
        let state = model.m9CommandState(command)
        return Button { model.performM9Pane(command) } label: { Image(systemName: symbol) }
            .help(language.commandLabel(command))
            .disabled(!state.enabled)
            .accessibilityLabel(language.commandLabel(command))
            .accessibilityIdentifier("inkpod.command.\(command.rawValue)")
    }
}

struct M9AnimationInspector: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if model.lightTableVisible { lightTableSection }
                if model.subpaletteVisible { subpaletteSection }
                if model.referenceVisible { referenceSection }
                if !model.lightTableVisible, !model.subpaletteVisible, !model.referenceVisible {
                    ContentUnavailableView(
                        language.text("m11.inspector.animation.empty"),
                        systemImage: "film.stack"
                    )
                }
            }
            .padding(8)
            .background(
                Color(nsColor: .controlBackgroundColor),
                in: RoundedRectangle(cornerRadius: 8)
            )
            .padding(8)
        }
        .frame(minWidth: 230, idealWidth: 270, maxWidth: 360)
        .accessibilityIdentifier("inkpod.m9.animation-inspector")
    }

    @ViewBuilder
    private var lightTableSection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(language.text("m9.lightTable.title")).font(.headline)
                    Spacer()
                    commandButton(.lightTablePin, "pin")
                    commandButton(.lightTableSetNew, "plus")
                    commandButton(.lightTableSetDelete, "trash")
                }
                Picker(
                    language.text("m9.lightTable.set"),
                    selection: Binding(
                        get: { model.selectedLightTableSetID ?? 0 },
                        set: { model.selectLightTableSet($0) }
                    )
                ) {
                    ForEach(model.lightTableAnimation?.lightTableSets ?? [], id: \.id) { set in
                        Text(set.name).tag(set.id)
                    }
                }
                .accessibilityLabel(language.text("m9.lightTable.set"))
                .accessibilityIdentifier("inkpod.m9.light-table-set")
                .accessibilityAdjustableAction { direction in
                    adjustSelectedLightTableSet(direction)
                }
                ForEach(model.selectedLightTableSet?.items ?? [], id: \.id) { item in
                    Button {
                        model.selectLightTableItem(item.id)
                    } label: {
                        HStack {
                            Image(systemName: item.isVisible ? "eye" : "eye.slash")
                            Text(item.name).lineLimit(1)
                            Spacer()
                            Text("\(item.effectiveOpacityMilli / 10)%")
                                .font(.caption.monospacedDigit())
                        }
                        .padding(4)
                        .background(
                            model.selectedLightTableItemID == item.id
                                ? Color.accentColor.opacity(0.18) : Color.clear,
                            in: RoundedRectangle(cornerRadius: 4)
                        )
                    }
                    .buttonStyle(.plain)
                }
                HStack {
                    commandButton(.lightTableItemAdd, "photo.badge.plus")
                    commandButton(.lightTableItemReload, "arrow.clockwise")
                    commandButton(.lightTableItemProperties, "slider.horizontal.3")
                    commandButton(.lightTableItemSwap, "arrow.triangle.swap")
                }
                HStack {
                    commandButton(.lightTableBulkPrevious, "chevron.left.2")
                    commandButton(.lightTableBulkBoth, "arrow.left.and.right")
                    commandButton(.lightTableBulkNext, "chevron.right.2")
                }
            }
        }
    }

    private func adjustSelectedLightTableSet(_ direction: AccessibilityAdjustmentDirection) {
        let sets = model.lightTableAnimation?.lightTableSets ?? []
        guard !sets.isEmpty else { return }
        let currentIndex = sets.firstIndex { $0.id == model.selectedLightTableSetID } ?? 0
        let nextIndex: Int
        switch direction {
        case .increment:
            nextIndex = min(currentIndex + 1, sets.index(before: sets.endIndex))
        case .decrement:
            nextIndex = max(currentIndex - 1, sets.startIndex)
        @unknown default:
            return
        }
        model.selectLightTableSet(sets[nextIndex].id)
    }

    @ViewBuilder
    private var subpaletteSection: some View {
        GroupBox(language.text("m9.subpalette.title")) {
            VStack(alignment: .leading, spacing: 6) {
                activeThumbnail
                HStack {
                    commandButton(.subpaletteSet, "paintpalette")
                    commandButton(.subpaletteSample, "eyedropper")
                    commandButton(.subpalettePin, "pin")
                }
            }
        }
    }

    @ViewBuilder
    private var referenceSection: some View {
        GroupBox(language.text("m9.reference.title")) {
            VStack(alignment: .leading, spacing: 6) {
                activeThumbnail
                Text(model.subpaletteAnimation?.sequence.first(where: {
                    $0.index == model.subpaletteAnimation?.activeSequenceIndex
                })?.name ?? language.text("m9.reference.empty"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var activeThumbnail: some View {
        if let cell = model.subpaletteAnimation?.sequence.first(where: {
            $0.index == model.subpaletteAnimation?.activeSequenceIndex
        }) {
            M9Thumbnail(
                width: cell.thumbnailWidth,
                height: cell.thumbnailHeight,
                rgba8: cell.thumbnailRGBA8
            )
            .frame(maxWidth: .infinity, minHeight: 96, maxHeight: 160)
        } else {
            ContentUnavailableView(
                language.text("m9.reference.empty"),
                systemImage: "photo"
            )
            .frame(height: 96)
        }
    }

    private func commandButton(_ command: InkpodCommandID, _ symbol: String) -> some View {
        let state = model.m9CommandState(command)
        return Button { model.performM9Pane(command) } label: { Image(systemName: symbol) }
            .help(language.commandLabel(command))
            .disabled(!state.enabled)
            .accessibilityLabel(language.commandLabel(command))
            .accessibilityIdentifier("inkpod.command.\(command.rawValue)")
    }
}

struct M9EditorSheet: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var draft: M9EditorDraft

    init(model: WorkspaceModel, draft: M9EditorDraft, language: AppLanguageController) {
        self.model = model
        self.language = language
        _draft = State(initialValue: draft)
    }

    var body: some View {
        Form {
            switch draft {
            case let .cut(value): cutForm(value)
            case let .renumber(value): renumberForm(value)
            case let .sequenceIndex(value): sequenceIndexForm(value)
            case let .setName(value): setNameForm(value)
            case let .opacity(value): opacityForm(value)
            case let .lightTableItem(value): itemForm(value)
            case let .bulk(value): bulkForm(value)
            }
            HStack {
                Button(language.text("action.cancel"), role: .cancel) {
                    model.cancelM9Editor()
                }
                Spacer()
                Button(language.text("action.apply")) {
                    model.applyM9Editor(draft)
                }
                .keyboardShortcut(.defaultAction)
                .disabled(!isValid)
            }
        }
        .padding()
        .frame(width: 480)
        .interactiveDismissDisabled()
        .accessibilityIdentifier("inkpod.m9.editor-sheet")
    }

    @ViewBuilder
    private func cutForm(_ value: M9CutEditorDraft) -> some View {
        Text(value.createsCut ? language.text("m9.cut.new") : language.text("m9.cut.properties"))
            .font(.headline)
        TextField(language.text("m9.cut.workTitle"), text: cutMetadata(\.workTitle))
        TextField(language.text("m9.cut.episode"), text: cutMetadata(\.episode))
        TextField(language.text("m9.cut.scene"), text: cutMetadata(\.scene))
        TextField(language.text("m9.cut.name"), text: cutMetadata(\.cutName))
        TextField(language.text("m9.cut.instruction"), text: cutMetadata(\.instruction))
        LabeledContent(language.text("m9.cut.duration")) {
            TextField("", value: cutMetadata(\.durationFrames), format: .number)
        }
        Picker(language.text("m5.field.sizing"), selection: cutSizingMode) {
            Text(language.text("m5.sizing.imagePixels"))
                .tag(CoreCellSizingMode.imagePixels)
            Text(language.text("m5.sizing.frameSize"))
                .tag(CoreCellSizingMode.frameMicrometres)
        }
        HStack {
            TextField("W", value: cutDefaults(\.width), format: .number)
            TextField("H", value: cutDefaults(\.height), format: .number)
            TextField("DPI X", value: cutDefaults(\.dpiXMilli), format: .number)
            TextField("DPI Y", value: cutDefaults(\.dpiYMilli), format: .number)
        }
        HStack {
            TextField(
                language.text("m5.field.margin"),
                value: cutDefaults(\.marginMilli),
                format: .number
            )
            TextField(
                language.text("m5.field.safeFrameRatio"),
                value: cutDefaults(\.safeFrameRatioMilli),
                format: .number
            )
            TextField(
                language.text("m5.field.maximumCloseRatio"),
                value: cutDefaults(\.maximumCloseRatioMilli),
                format: .number
            )
        }
        if value.createsCut {
            TextField(
                language.text("m5.field.cellCount"),
                value: cutCellCount,
                format: .number
            )
        }
        Picker(language.text("m5.field.anchor"), selection: cutAnchor) {
            ForEach(CoreFrameAnchor.allCases, id: \.self) {
                Text(cutAnchorName($0)).tag($0)
            }
        }
        Picker(language.text("m5.field.initialLayer"), selection: cutInitialLayerKind) {
            ForEach(CoreLayerKind.allCases, id: \.self) {
                Text(language.text("m5.layerKind.\($0.rawValue)")).tag($0)
            }
        }
        Picker(language.text("m5.field.pixelFormat"), selection: cutPixelFormat) {
            ForEach([CorePixelStorageFormat.rgba8, .rgba16], id: \.self) {
                Text(language.text("m5.pixelFormat.\($0.rawValue)")).tag($0)
            }
        }
    }

    @ViewBuilder
    private func renumberForm(_ value: M9RenumberDraft) -> some View {
        Text(language.commandLabel(.cutSequenceRenumber)).font(.headline)
        TextField(language.text("m9.renumber.position"), value: renumber(\.position), format: .number)
        TextField(language.text("m9.renumber.count"), value: renumber(\.count), format: .number)
        TextField(language.text("m9.renumber.first"), value: renumber(\.first), format: .number)
        TextField(language.text("m9.renumber.step"), value: renumber(\.step), format: .number)
    }

    @ViewBuilder
    private func sequenceIndexForm(_ value: M9IndexDraft) -> some View {
        Text(language.commandLabel(.sequenceGoto)).font(.headline)
        TextField(language.text("m9.sequence.index"), value: sequenceIndex, format: .number)
    }

    @ViewBuilder
    private func setNameForm(_ value: M9NameDraft) -> some View {
        Text(language.commandLabel(.lightTableSetRename)).font(.headline)
        TextField(language.text("m9.lightTable.name"), text: setName)
    }

    @ViewBuilder
    private func opacityForm(_ value: M9OpacityDraft) -> some View {
        Text(language.commandLabel(.lightTableGlobalOpacity)).font(.headline)
        Slider(value: opacity, in: 0 ... 1_000, step: 1)
        Text("\(value.opacityMilli / 10)%").font(.body.monospacedDigit())
    }

    @ViewBuilder
    private func itemForm(_ value: M9LightTableItemDraft) -> some View {
        Text(language.commandLabel(.lightTableItemProperties)).font(.headline)
        TextField(language.text("m9.lightTable.name"), text: item(\.name))
        Toggle(language.text("m9.lightTable.visible"), isOn: item(\.isVisible))
        Picker(language.text("m9.lightTable.mode"), selection: item(\.displayMode)) {
            Text(language.text("m9.lightTable.color")).tag(CoreLightTableDisplayMode.color)
            Text(language.text("m9.lightTable.monotone")).tag(CoreLightTableDisplayMode.monotone)
            Text(language.text("m9.lightTable.halftone")).tag(CoreLightTableDisplayMode.halftone)
        }
        ColorPicker(
            language.text("m9.lightTable.displayColor"),
            selection: Binding(
                get: { value.displayColor.swiftUIColor },
                set: { item(\.displayColor).wrappedValue = CoreColorValue(
                    $0,
                    preserving: value.displayColor.depth
                ) }
            ),
            supportsOpacity: true
        )
        Slider(value: itemDouble(\.opacityMilli), in: 0 ... 1_000, step: 1)
        HStack {
            TextField("X", value: item(\.translateXMilli), format: .number)
            TextField("Y", value: item(\.translateYMilli), format: .number)
            TextField("R", value: item(\.rotationMilliDegrees), format: .number)
        }
        HStack {
            TextField("Scale X", value: item(\.scaleXMilli), format: .number)
            TextField("Scale Y", value: item(\.scaleYMilli), format: .number)
        }
    }

    @ViewBuilder
    private func bulkForm(_ value: M9LightTableBulkDraft) -> some View {
        Text(language.text("m9.bulk.title")).font(.headline)
        Text(language.text("m9.bulk.summary")
            .replacingOccurrences(of: "%1$@", with: String(value.preview.addCount))
            .replacingOccurrences(of: "%2$@", with: String(value.preview.skipCount)))
        ForEach(Array(value.preview.entries.enumerated()), id: \.offset) { _, entry in
            let action = entry.action == .add
                ? language.text("m9.bulk.add") : language.text("m9.bulk.skip")
            Text(verbatim: "#\(entry.cellNumber) · \(entry.opacityMilli / 10)% · \(action)")
                .font(.caption.monospacedDigit())
        }
    }

    private var isValid: Bool {
        switch draft {
        case let .cut(value):
            !value.metadata.cutName.isEmpty && value.metadata.durationFrames > 0
                && value.defaults.width > 0 && value.defaults.height > 0
                && value.defaults.dpiXMilli > 0 && value.defaults.dpiYMilli > 0
                && value.defaults.safeFrameRatioMilli <= 1_000
                && value.defaults.maximumCloseRatioMilli <= 1_000
                && (!value.createsCut || (1 ... 64).contains(value.cellCount))
        case let .renumber(value): value.count > 0 && value.first > 0 && value.step > 0
        case let .sequenceIndex(value):
            Int(value.index) < (model.m9Animation(for: value.context)?.sequence.count ?? 0)
        case let .setName(value): !value.name.isEmpty && value.name.utf8.count <= 4_096
        case let .opacity(value): value.opacityMilli <= 1_000
        case let .lightTableItem(value):
            !value.name.isEmpty && value.opacityMilli <= 1_000
                && value.scaleXMilli > 0 && value.scaleYMilli > 0
        case .bulk: true
        }
    }

    private func cutMetadata<Value>(
        _ keyPath: WritableKeyPath<CoreCutMetadata, Value>
    ) -> Binding<Value> {
        Binding(
            get: { if case let .cut(value) = draft { value.metadata[keyPath: keyPath] } else { fatalError() } },
            set: { newValue in
                guard case var .cut(value) = draft else { return }
                value.metadata[keyPath: keyPath] = newValue
                draft = .cut(value)
            }
        )
    }

    private func cutDefaults<Value>(
        _ keyPath: WritableKeyPath<CoreCutDefaults, Value>
    ) -> Binding<Value> {
        Binding(
            get: { if case let .cut(value) = draft { value.defaults[keyPath: keyPath] } else { fatalError() } },
            set: { newValue in
                guard case var .cut(value) = draft else { return }
                value.defaults[keyPath: keyPath] = newValue
                draft = .cut(value)
            }
        )
    }

    private var cutCellCount: Binding<UInt32> {
        Binding(
            get: { if case let .cut(value) = draft { value.cellCount } else { 1 } },
            set: { newValue in
                guard case var .cut(value) = draft else { return }
                value.cellCount = newValue
                draft = .cut(value)
            }
        )
    }

    private var cutSizingMode: Binding<CoreCellSizingMode> {
        cutDefaultEnum(\.sizingMode, fallback: .imagePixels)
    }

    private var cutAnchor: Binding<CoreFrameAnchor> {
        cutDefaultEnum(\.anchor, fallback: .center)
    }

    private var cutInitialLayerKind: Binding<CoreLayerKind> {
        Binding(
            get: {
                if case let .cut(value) = draft { value.defaults.initialLayerKind }
                else { .raster }
            },
            set: { newValue in
                guard case var .cut(value) = draft else { return }
                value.defaults.initialLayerKind = newValue
                draft = .cut(value)
            }
        )
    }

    private var cutPixelFormat: Binding<CorePixelStorageFormat> {
        Binding(
            get: {
                if case let .cut(value) = draft { value.defaults.pixelFormat }
                else { .rgba8 }
            },
            set: { newValue in
                guard case var .cut(value) = draft else { return }
                value.defaults.pixelFormat = newValue
                draft = .cut(value)
            }
        )
    }

    private func cutDefaultEnum<Value: RawRepresentable>(
        _ keyPath: WritableKeyPath<CoreCutDefaults, UInt32>,
        fallback: Value
    ) -> Binding<Value> where Value.RawValue == UInt32 {
        Binding(
            get: {
                guard case let .cut(value) = draft else { return fallback }
                return Value(rawValue: value.defaults[keyPath: keyPath]) ?? fallback
            },
            set: { newValue in
                guard case var .cut(value) = draft else { return }
                value.defaults[keyPath: keyPath] = newValue.rawValue
                draft = .cut(value)
            }
        )
    }

    private func cutAnchorName(_ anchor: CoreFrameAnchor) -> String {
        switch anchor {
        case .topLeft: language.text("m5.anchor.topLeft")
        case .topRight: language.text("m5.anchor.topRight")
        case .center: language.text("m5.anchor.center")
        case .bottomLeft: language.text("m5.anchor.bottomLeft")
        case .bottomRight: language.text("m5.anchor.bottomRight")
        }
    }

    private func renumber<Value>(
        _ keyPath: WritableKeyPath<M9RenumberDraft, Value>
    ) -> Binding<Value> {
        Binding(
            get: { if case let .renumber(value) = draft { value[keyPath: keyPath] } else { fatalError() } },
            set: { newValue in
                guard case var .renumber(value) = draft else { return }
                value[keyPath: keyPath] = newValue
                draft = .renumber(value)
            }
        )
    }

    private var sequenceIndex: Binding<UInt32> {
        Binding(
            get: { if case let .sequenceIndex(value) = draft { value.index } else { 0 } },
            set: { value in
                guard case var .sequenceIndex(current) = draft else { return }
                current.index = value
                draft = .sequenceIndex(current)
            }
        )
    }

    private var setName: Binding<String> {
        Binding(
            get: { if case let .setName(value) = draft { value.name } else { "" } },
            set: { value in
                guard case var .setName(current) = draft else { return }
                current.name = value
                draft = .setName(current)
            }
        )
    }

    private var opacity: Binding<Double> {
        Binding(
            get: { if case let .opacity(value) = draft { Double(value.opacityMilli) } else { 0 } },
            set: { value in
                guard case var .opacity(current) = draft else { return }
                current.opacityMilli = UInt32(clamping: Int(value.rounded()))
                draft = .opacity(current)
            }
        )
    }

    private func item<Value>(
        _ keyPath: WritableKeyPath<M9LightTableItemDraft, Value>
    ) -> Binding<Value> {
        Binding(
            get: { if case let .lightTableItem(value) = draft { value[keyPath: keyPath] } else { fatalError() } },
            set: { newValue in
                guard case var .lightTableItem(value) = draft else { return }
                value[keyPath: keyPath] = newValue
                draft = .lightTableItem(value)
            }
        )
    }

    private func itemDouble(
        _ keyPath: WritableKeyPath<M9LightTableItemDraft, UInt32>
    ) -> Binding<Double> {
        Binding(
            get: { if case let .lightTableItem(value) = draft { Double(value[keyPath: keyPath]) } else { 0 } },
            set: { newValue in
                guard case var .lightTableItem(value) = draft else { return }
                value[keyPath: keyPath] = UInt32(clamping: Int(newValue.rounded()))
                draft = .lightTableItem(value)
            }
        )
    }
}

struct M9Thumbnail: View {
    let width: UInt32
    let height: UInt32
    let rgba8: [UInt8]

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.none)
                    .scaledToFit()
            } else {
                Color.clear.overlay(Image(systemName: "photo"))
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 4))
    }

    private var image: NSImage? {
        guard width > 0, height > 0,
              UInt64(width) * UInt64(height) * 4 == UInt64(rgba8.count),
              let provider = CGDataProvider(data: Data(rgba8) as CFData),
              let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
              let cgImage = CGImage(
                  width: Int(width),
                  height: Int(height),
                  bitsPerComponent: 8,
                  bitsPerPixel: 32,
                  bytesPerRow: Int(width) * 4,
                  space: colorSpace,
                  bitmapInfo: CGBitmapInfo(
                      rawValue: CGImageAlphaInfo.last.rawValue
                  ),
                  provider: provider,
                  decode: nil,
                  shouldInterpolate: false,
                  intent: .defaultIntent
              )
        else { return nil }
        return NSImage(
            cgImage: cgImage,
            size: NSSize(width: CGFloat(width), height: CGFloat(height))
        )
    }
}

private extension PaneTargetRecord {
    var isPinned: Bool {
        if case .pinnedDocument = mode { return true }
        return false
    }
}
