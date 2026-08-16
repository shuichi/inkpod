import AppKit
import SwiftUI

struct M6ToolSidebar: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    private let tools: [(InkpodCommandID, String)] = [
        (.toolPencil, "pencil"),
        (.toolBrush, "paintbrush"),
        (.toolEraser, "eraser"),
        (.toolFill, "paintbrush.pointed.fill"),
        (.toolEyedropper, "eyedropper"),
        (.selectionRectangle, "rectangle.dashed"),
        (.toolColorReplaceTarget, "arrow.triangle.2.circlepath"),
    ]

    var body: some View {
        VStack(spacing: 8) {
            ForEach(tools, id: \.0) { command, symbol in
                commandButton(command, symbol: symbol)
            }
            Divider()
            if model.toolOptionsVisible, let editor = model.paint?.editor {
                VStack(spacing: 4) {
                    Text(language.text("m6.tools.size"))
                        .font(.caption)
                    Slider(
                        value: Binding(
                            get: { editor.diameter },
                            set: { model.updateEditor(.diameter($0)) }
                        ),
                        in: 1 ... 256
                    )
                    .frame(width: 136)
                    Text(String(format: "%.1f", editor.diameter))
                        .font(.caption.monospacedDigit())
                }
                if editor.activeTool == .fill {
                    commandButton(.toolClosedFill, symbol: "square.dashed")
                    commandButton(.toolFillExtension, symbol: "arrow.up.left.and.down.right.magnifyingglass")
                    fillOptions(editor.fillOptions)
                }
                if editor.activeTool == .brush || editor.activeTool == .pencil {
                    brushOptions(editor.brushOptions)
                }
                if editor.activeTool == .colorReplace {
                    commandButton(.toolColorReplaceRectangle, symbol: "rectangle.dashed")
                    commandButton(.toolColorReplacePen, symbol: "scribble.variable")
                    commandButton(.toolColorReplaceLasso, symbol: "lasso")
                    commandButton(.toolColorReplaceAll, symbol: "rectangle.inset.filled")
                    if let preview = model.colorReplacePreview {
                        Text(language.text("m6.replace.preview"))
                            .font(.caption.weight(.semibold))
                        Text("\(preview.matchedPixels)")
                            .font(.caption.monospacedDigit())
                    }
                }
                if editor.activeTool == .selection {
                    selectionOptions(editor.selectionOptions)
                }
            }
            Spacer()
        }
        .padding(7)
        .frame(width: model.toolOptionsVisible ? 178 : 54)
        .accessibilityIdentifier("inkpod.m6.tool-sidebar")
    }

    private func commandButton(_ command: InkpodCommandID, symbol: String) -> some View {
        let state = model.commandContext.map { model.commandState(command, context: $0) }
            ?? CommandState(enabled: false)
        return Button { model.perform(command) } label: {
            Image(systemName: symbol)
                .frame(width: 28, height: 28)
                .background(
                    state.checked ? Color.accentColor.opacity(0.24) : Color.clear,
                    in: RoundedRectangle(cornerRadius: 6)
                )
        }
        .buttonStyle(.borderless)
        .disabled(!state.enabled)
        .help(language.commandLabel(command))
        .accessibilityLabel(language.commandLabel(command))
        .accessibilityIdentifier("inkpod.command.\(command.rawValue)")
        .accessibilityAddTraits(state.checked ? .isSelected : [])
    }

    @ViewBuilder
    private func fillOptions(_ options: CoreFillOptions) -> some View {
        Text(language.text("m6.fill.tolerance")).font(.caption)
        Slider(
            value: Binding(
                get: { Double(options.tolerance) },
                set: { model.updateEditor(.fillOptions(options.withTolerance(UInt16($0)))) }
            ),
            in: 0 ... 65_535,
            step: 256
        )
        Text("\(options.tolerance)").font(.caption.monospacedDigit())
        Text(language.text("m6.fill.gapClose")).font(.caption)
        Slider(
            value: Binding(
                get: { Double(options.gapClose) },
                set: { model.updateEditor(.fillOptions(options.withGapClose(UInt16($0)))) }
            ),
            in: 0 ... 255,
            step: 1
        )
        Text("\(options.gapClose)").font(.caption.monospacedDigit())
        Toggle(
            language.text("m6.fill.selection"),
            isOn: Binding(
                get: { options.useDocumentSelection },
                set: { model.updateEditor(.fillOptions(options.withSelection($0))) }
            )
        )
        Toggle(
            language.text("m6.fill.transparentOnly"),
            isOn: Binding(
                get: { options.transparentOnly },
                set: { model.updateEditor(.fillOptions(options.withTransparentOnly($0))) }
            )
        )
        Toggle(
            language.text("m6.fill.detachedRegions"),
            isOn: Binding(
                get: { options.detachedRegions },
                set: { model.updateEditor(.fillOptions(options.withDetachedRegions($0))) }
            )
        )
    }

    @ViewBuilder
    private func brushOptions(_ options: CoreBrushOptions) -> some View {
        Picker(
            language.text("m6.brush.shape"),
            selection: Binding(
                get: { options.shape },
                set: { model.updateEditor(.brushOptions(options.withShape($0))) }
            )
        ) {
            Text(language.text("m6.brush.round")).tag(CoreBrushShape.round)
            Text(language.text("m6.brush.square")).tag(CoreBrushShape.square)
        }
        Text(language.text("m6.brush.smoothing")).font(.caption)
        Slider(
            value: Binding(
                get: { Double(options.smoothing) },
                set: { model.updateEditor(.brushOptions(options.withSmoothing(UInt16($0)))) }
            ),
            in: 0 ... 1_000,
            step: 10
        )
        Toggle(
            language.text("m6.brush.startColor"),
            isOn: Binding(
                get: { options.startColor == .exactNative },
                set: {
                    model.updateEditor(.brushOptions(
                        options.withStartColor($0 ? .exactNative : .any)
                    ))
                }
            )
        )
    }

    @ViewBuilder
    private func selectionOptions(_ options: CoreSelectionOptions) -> some View {
        Picker(
            language.text("selection.option.shape"),
            selection: Binding(
                get: { options.shape },
                set: { model.updateEditor(.selectionOptions(options.withShape($0))) }
            )
        ) {
            Text(language.commandLabel(.selectionRectangle)).tag(CoreSelectionShape.rectangle)
            Text(language.commandLabel(.selectionEllipse)).tag(CoreSelectionShape.ellipse)
            Text(language.commandLabel(.selectionLasso)).tag(CoreSelectionShape.lasso)
            Text(language.commandLabel(.selectionPolyline)).tag(CoreSelectionShape.polyline)
            Text(language.commandLabel(.selectionTrace)).tag(CoreSelectionShape.trace)
            Text(language.commandLabel(.selectionWand)).tag(CoreSelectionShape.wand)
        }
        Picker(
            language.text("selection.option.mode"),
            selection: Binding(
                get: { options.operation },
                set: { model.updateEditor(.selectionOptions(options.withOperation($0))) }
            )
        ) {
            Text(language.commandLabel(.selectionModeNew)).tag(CoreSelectionOperation.replace)
            Text(language.commandLabel(.selectionModeAdd)).tag(CoreSelectionOperation.add)
            Text(language.commandLabel(.selectionModeSubtract)).tag(CoreSelectionOperation.subtract)
            Text(language.commandLabel(.selectionModeIntersect)).tag(CoreSelectionOperation.intersect)
        }
        Text(language.text("selection.option.tolerance")).font(.caption)
        Slider(
            value: Binding(
                get: { Double(options.tolerance) },
                set: { model.updateEditor(.selectionOptions(options.withTolerance(UInt16($0)))) }
            ),
            in: 0 ... 65_535,
            step: 256
        )
        Text(language.text("selection.option.gapClose")).font(.caption)
        Slider(
            value: Binding(
                get: { Double(options.gapClose) },
                set: { model.updateEditor(.selectionOptions(options.withGapClose(UInt16($0)))) }
            ),
            in: 0 ... 255,
            step: 1
        )
        if options.shape == .trace {
            Text(language.text("selection.option.diameter")).font(.caption)
            Slider(
                value: Binding(
                    get: { options.diameter },
                    set: { model.updateEditor(.selectionOptions(options.withDiameter($0))) }
                ),
                in: 1 ... 256
            )
            Toggle(
                language.text("selection.option.pressure"),
                isOn: Binding(
                    get: { options.pressureControlsSize },
                    set: { model.updateEditor(.selectionOptions(options.withPressure($0))) }
                )
            )
        }
        Toggle(
            language.text("selection.option.fromCenter"),
            isOn: Binding(
                get: { options.fromCenter },
                set: { model.updateEditor(.selectionOptions(options.withFromCenter($0))) }
            )
        )
        Toggle(
            language.text("selection.option.rotate45"),
            isOn: Binding(
                get: { options.constrainRotationTo45Degrees },
                set: { model.updateEditor(.selectionOptions(options.withRotate45($0))) }
            )
        )
    }
}

struct M6ColorInspector: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if model.colorInspectorVisible {
                    colorSection
                    paletteSection
                    chartSection
                }
                if model.locatorVisible {
                    locatorSection
                }
            }
            .padding(10)
            .background(
                Color(nsColor: .controlBackgroundColor),
                in: RoundedRectangle(cornerRadius: 8)
            )
            .padding(8)
        }
        .frame(minWidth: 230, idealWidth: 270, maxWidth: 360)
        .accessibilityIdentifier("inkpod.m6.color-inspector")
    }

    @ViewBuilder
    private var colorSection: some View {
        HStack {
            Text(language.text("m6.color.title"))
                .font(.headline)
                .accessibilityIdentifier("inkpod.m6.color-section")
            Spacer()
            pinButton(pinned: model.colorPaneIsPinned) { model.toggleColorPanePin() }
        }
        if let color = model.paint?.editor.currentColor {
            ColorPicker(
                language.text("m6.color.current"),
                selection: Binding(
                    get: { color.swiftUIColor },
                    set: { model.chooseColor(CoreColorValue($0, preserving: color.depth)) }
                ),
                supportsOpacity: true
            )
            .accessibilityIdentifier("inkpod.m6.color-picker")
            HStack {
                swatch(color)
                Text(color.componentDescription)
                    .font(.caption.monospacedDigit())
                    .textSelection(.enabled)
            }
            if model.paint?.editor.activeTool == .colorReplace {
                ColorPicker(
                    language.text("m6.color.replaceTarget"),
                    selection: Binding(
                        get: { model.colorReplaceTarget.swiftUIColor },
                        set: {
                            model.colorReplaceTarget = CoreColorValue(
                                $0,
                                preserving: model.colorReplaceTarget.depth
                            )
                        }
                    ),
                    supportsOpacity: true
                )
                .accessibilityIdentifier("inkpod.m6.color-replace-target")
            }
        }
        Picker(
            language.text("m6.color.eyedropperSource"),
            selection: $model.eyedropperSource
        ) {
            Text(language.commandLabel(.colorSourceTopmost))
                .tag(CoreEyedropperSource.topmostNontransparent)
            Text(language.commandLabel(.colorSourceSelected))
                .tag(CoreEyedropperSource.selectedPlane)
            Text(language.commandLabel(.colorSourceComposite))
                .tag(CoreEyedropperSource.composite)
            Text(language.commandLabel(.colorSourceLightTable))
                .tag(CoreEyedropperSource.lightTableTopmost)
        }
        Picker(
            language.text("m6.color.check"),
            selection: colorCheckBinding
        ) {
            Text(language.commandLabel(.colorCheckOff)).tag(CoreColorCheckMode.off)
            Text(language.commandLabel(.colorCheckLegacy)).tag(CoreColorCheckMode.legacyWhite)
            Text(language.commandLabel(.colorCheckNative)).tag(CoreColorCheckMode.nativeAlpha)
        }
        Divider()
    }

    @ViewBuilder
    private var paletteSection: some View {
        Text(language.text("m6.palette.title"))
            .font(.headline)
            .accessibilityIdentifier("inkpod.m6.palette-section")
        let allColors = model.paint?.palette.colors ?? []
        let colors = Array(allColors.dropFirst(Int(model.palettePage) * 16).prefix(16))
        LazyVGrid(columns: Array(repeating: GridItem(.fixed(25)), count: 8), spacing: 5) {
            ForEach(Array(colors.enumerated()), id: \.offset) { index, color in
                Button { model.choosePaletteColor(color) } label: {
                    swatch(color).frame(width: 23, height: 23)
                }
                .buttonStyle(.plain)
                .help("\(index + 1): \(color.componentDescription)")
            }
        }
        HStack {
            smallCommand(.paletteRegister, symbol: "plus")
            smallCommand(.paletteDelete, symbol: "minus")
            smallCommand(.paletteClear, symbol: "trash")
            smallCommand(.paletteLoad, symbol: "folder")
            smallCommand(.paletteSave, symbol: "square.and.arrow.down")
        }
        Divider()
    }

    @ViewBuilder
    private var chartSection: some View {
        Text(language.text("m6.chart.title"))
            .font(.headline)
            .accessibilityIdentifier("inkpod.m6.chart-section")
        TextField(language.text("m6.chart.search"), text: $model.chartSearchText)
            .onSubmit { model.perform(.chartNext) }
        let entries = filteredChartEntries
        VStack(spacing: 2) {
            ForEach(Array(entries.dropFirst(Int(model.chartPage) * 32).prefix(32))) { entry in
                Button {
                    model.selectChartEntry(entry)
                } label: {
                    HStack {
                        swatch(entry.color).frame(width: 18, height: 18)
                        Text(entry.name).lineLimit(1)
                        Spacer()
                        if let frequency = entry.frequency {
                            Text("\(frequency)").font(.caption.monospacedDigit())
                        }
                    }
                }
                .buttonStyle(.plain)
            }
        }
        HStack {
            smallCommand(.chartGenerate, symbol: "wand.and.stars")
            smallCommand(.chartLock, symbol: model.paint?.chart.isLocked == true ? "lock.fill" : "lock.open")
            smallCommand(.chartLoad, symbol: "folder")
            smallCommand(.chartSave, symbol: "square.and.arrow.down")
        }
        if let preview = model.colorChartPreview {
            VStack(alignment: .leading, spacing: 6) {
                Text(language.text("m6.chart.preview"))
                    .font(.subheadline.weight(.semibold))
                Text("\(preview.retainedColorCount) / \(preview.sourceUniqueColorCount)")
                    .font(.caption.monospacedDigit())
                HStack {
                    Button(language.text("action.cancel")) {
                        model.cancelColorChartPreview()
                    }
                    Button(language.text("action.apply")) {
                        model.applyColorChartPreview()
                    }
                    .keyboardShortcut(.defaultAction)
                }
            }
            .padding(8)
            .background(Color(nsColor: .windowBackgroundColor), in: RoundedRectangle(cornerRadius: 6))
            .accessibilityIdentifier("inkpod.m6.chart-preview")
        }
        Divider()
    }

    @ViewBuilder
    private var locatorSection: some View {
        HStack {
            Text(language.text("m6.locator.title"))
                .font(.headline)
                .accessibilityIdentifier("inkpod.m6.locator-section")
            Spacer()
            pinButton(pinned: model.locatorPaneIsPinned) { model.toggleLocatorPanePin() }
        }
        Toggle(language.text("m6.locator.fixed"), isOn: $model.locatorFixed)
        Toggle(language.text("m6.locator.autoscroll"), isOn: $model.locatorAutoscroll)
        if let locator = model.locator {
            Text("x \(locator.documentX), y \(locator.documentY)")
                .font(.body.monospacedDigit())
            if let color = locator.color {
                HStack { swatch(color); Text(color.componentDescription) }
            }
            LocatorNeighborhood(model: model, projection: locator)
                .frame(width: 126, height: 126)
                .accessibilityLabel(language.text("m6.locator.neighborhood"))
        } else {
            Text(language.text("m6.locator.movePointer"))
                .foregroundStyle(.secondary)
        }
    }

    private var filteredChartEntries: [CoreColorChartEntry] {
        let entries = model.paint?.chart.entries ?? []
        guard !model.chartSearchText.isEmpty else { return entries }
        return entries.filter { $0.name.localizedCaseInsensitiveContains(model.chartSearchText) }
    }

    private var colorCheckBinding: Binding<CoreColorCheckMode> {
        Binding(
            get: { model.paint?.colorCheckMode ?? .off },
            set: { mode in
                let command: InkpodCommandID = switch mode {
                case .off: .colorCheckOff
                case .legacyWhite: .colorCheckLegacy
                case .nativeAlpha: .colorCheckNative
                }
                model.perform(command)
            }
        )
    }

    private func swatch(_ color: CoreColorValue) -> some View {
        RoundedRectangle(cornerRadius: 3)
            .fill(color.swiftUIColor)
            .overlay(RoundedRectangle(cornerRadius: 3).stroke(.separator))
            .frame(width: 28, height: 20)
    }

    private func smallCommand(_ command: InkpodCommandID, symbol: String) -> some View {
        let state = model.commandContext.map { model.commandState(command, context: $0) }
            ?? CommandState(enabled: false)
        return Button { model.perform(command) } label: { Image(systemName: symbol) }
            .buttonStyle(.borderless)
            .disabled(!state.enabled)
            .help(language.commandLabel(command))
            .accessibilityLabel(language.commandLabel(command))
            .accessibilityIdentifier("inkpod.command.\(command.rawValue)")
    }

    private func pinButton(pinned: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: pinned ? "pin.fill" : "pin") }
            .buttonStyle(.borderless)
            .accessibilityLabel(language.text(pinned ? "m6.pane.follow" : "m6.pane.pin"))
    }
}

private struct LocatorNeighborhood: View {
    @ObservedObject var model: WorkspaceModel
    let projection: CoreLocatorProjection

    var body: some View {
        Canvas { context, size in
            let width = Int(projection.neighborhoodWidth)
            let height = Int(projection.neighborhoodHeight)
            guard width > 0, height > 0,
                  projection.neighborhoodRGBA8.count == width * height * 4
            else { return }
            let cellWidth = size.width / CGFloat(width)
            let cellHeight = size.height / CGFloat(height)
            for y in 0 ..< height {
                for x in 0 ..< width {
                    let offset = (y * width + x) * 4
                    let color = Color(
                        .sRGB,
                        red: Double(projection.neighborhoodRGBA8[offset]) / 255,
                        green: Double(projection.neighborhoodRGBA8[offset + 1]) / 255,
                        blue: Double(projection.neighborhoodRGBA8[offset + 2]) / 255,
                        opacity: Double(projection.neighborhoodRGBA8[offset + 3]) / 255
                    )
                    context.fill(
                        Path(CGRect(
                            x: CGFloat(x) * cellWidth,
                            y: CGFloat(y) * cellHeight,
                            width: cellWidth + 0.5,
                            height: cellHeight + 0.5
                        )),
                        with: .color(color)
                    )
                }
            }
            let center = CGRect(
                x: CGFloat(width / 2) * cellWidth,
                y: CGFloat(height / 2) * cellHeight,
                width: cellWidth,
                height: cellHeight
            )
            context.stroke(Path(center), with: .color(.primary), lineWidth: 1.5)
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0)
                .onEnded { value in
                    guard model.locatorFixed,
                          projection.neighborhoodWidth > 0,
                          projection.neighborhoodHeight > 0
                    else { return }
                    let x = min(
                        Int(projection.neighborhoodWidth) - 1,
                        max(0, Int(value.location.x / 126
                            * CGFloat(projection.neighborhoodWidth)))
                    )
                    let y = min(
                        Int(projection.neighborhoodHeight) - 1,
                        max(0, Int(value.location.y / 126
                            * CGFloat(projection.neighborhoodHeight)))
                    )
                    model.selectLocatorPixel(
                        documentX: projection.neighborhoodOriginX + Int32(x),
                        documentY: projection.neighborhoodOriginY + Int32(y)
                    )
                }
        )
    }
}

private extension CoreFillOptions {
    func withTolerance(_ value: UInt16) -> CoreFillOptions { copy(tolerance: value) }
    func withGapClose(_ value: UInt16) -> CoreFillOptions { copy(gapClose: value) }
    func withSelection(_ value: Bool) -> CoreFillOptions {
        copy(useDocumentSelection: value)
    }
    func withTransparentOnly(_ value: Bool) -> CoreFillOptions {
        copy(transparentOnly: value)
    }
    func withDetachedRegions(_ value: Bool) -> CoreFillOptions {
        copy(detachedRegions: value)
    }

    private func copy(
        detachedRegions: Bool? = nil,
        transparentOnly: Bool? = nil,
        useDocumentSelection: Bool? = nil,
        tolerance: UInt16? = nil,
        gapClose: UInt16? = nil
    ) -> CoreFillOptions {
        CoreFillOptions(
            operation: operation,
            detachedRegions: detachedRegions ?? self.detachedRegions,
            overflowAbort: overflowAbort,
            transparentOnly: transparentOnly ?? self.transparentOnly,
            useDocumentSelection: useDocumentSelection ?? self.useDocumentSelection,
            useLightTableBoundary: useLightTableBoundary,
            useLightTableColor: useLightTableColor,
            tolerance: tolerance ?? self.tolerance,
            gapClose: gapClose ?? self.gapClose,
            inclusionMode: inclusionMode,
            extensionDistance: extensionDistance,
            inclusionColors: inclusionColors
        )
    }
}

private extension CoreBrushOptions {
    func withShape(_ value: CoreBrushShape) -> CoreBrushOptions {
        CoreBrushOptions(shape: value, smoothing: smoothing, startColor: startColor)
    }

    func withSmoothing(_ value: UInt16) -> CoreBrushOptions {
        CoreBrushOptions(shape: shape, smoothing: value, startColor: startColor)
    }

    func withStartColor(_ value: CoreStartColorPredicate) -> CoreBrushOptions {
        CoreBrushOptions(shape: shape, smoothing: smoothing, startColor: value)
    }
}

private extension CoreSelectionOptions {
    func withShape(_ value: CoreSelectionShape) -> CoreSelectionOptions { copy(shape: value) }
    func withOperation(_ value: CoreSelectionOperation) -> CoreSelectionOptions {
        copy(operation: value)
    }
    func withTolerance(_ value: UInt16) -> CoreSelectionOptions { copy(tolerance: value) }
    func withGapClose(_ value: UInt16) -> CoreSelectionOptions { copy(gapClose: value) }
    func withDiameter(_ value: Double) -> CoreSelectionOptions { copy(diameter: value) }
    func withPressure(_ value: Bool) -> CoreSelectionOptions {
        copy(pressureControlsSize: value, screenSizedTrace: value ? false : nil)
    }
    func withFromCenter(_ value: Bool) -> CoreSelectionOptions { copy(fromCenter: value) }
    func withRotate45(_ value: Bool) -> CoreSelectionOptions {
        copy(constrainRotationTo45Degrees: value)
    }

    private func copy(
        shape: CoreSelectionShape? = nil,
        operation: CoreSelectionOperation? = nil,
        tolerance: UInt16? = nil,
        gapClose: UInt16? = nil,
        diameter: Double? = nil,
        fromCenter: Bool? = nil,
        constrainRotationTo45Degrees: Bool? = nil,
        pressureControlsSize: Bool? = nil,
        screenSizedTrace: Bool? = nil
    ) -> CoreSelectionOptions {
        CoreSelectionOptions(
            shape: shape ?? self.shape,
            operation: operation ?? self.operation,
            tolerance: tolerance ?? self.tolerance,
            gapClose: gapClose ?? self.gapClose,
            diameter: diameter ?? self.diameter,
            interpretation: interpretation,
            aspectRatio: aspectRatio,
            fromCenter: fromCenter ?? self.fromCenter,
            constrainRotationTo45Degrees: constrainRotationTo45Degrees
                ?? self.constrainRotationTo45Degrees,
            pressureControlsSize: pressureControlsSize ?? self.pressureControlsSize,
            screenSizedTrace: screenSizedTrace ?? self.screenSizedTrace,
            rotationTurns: rotationTurns,
            traceShape: traceShape
        )
    }
}

extension CoreColorValue {
    init(_ color: Color, preserving depth: CoreColorDepth) {
        let converted = NSColor(color).usingColorSpace(.sRGB) ?? NSColor(color)
        let maximum: Double = switch depth {
        case .binary: 1
        case .grayscale8, .rgba8: 255
        case .grayscale16, .rgba16: 65_535
        }
        func component(_ value: CGFloat) -> UInt16 {
            UInt16(clamping: Int((min(max(Double(value), 0), 1) * maximum).rounded()))
        }
        let alpha = component(converted.alphaComponent)
        switch depth {
        case .binary:
            let value: UInt16 = converted.brightnessComponent >= 0.5 ? 1 : 0
            self.init(depth: depth, red: value, green: value, blue: value, alpha: alpha)
        case .grayscale8, .grayscale16:
            let value = component(converted.brightnessComponent)
            self.init(depth: depth, red: value, green: value, blue: value, alpha: alpha)
        case .rgba8, .rgba16:
            self.init(
                depth: depth,
                red: component(converted.redComponent),
                green: component(converted.greenComponent),
                blue: component(converted.blueComponent),
                alpha: alpha
            )
        }
    }

    var swiftUIColor: Color {
        let maximum: Double = switch depth {
        case .binary: 1
        case .grayscale8, .rgba8: 255
        case .grayscale16, .rgba16: 65_535
        }
        return Color(
            .sRGB,
            red: Double(red) / maximum,
            green: Double(green) / maximum,
            blue: Double(blue) / maximum,
            opacity: Double(alpha) / maximum
        )
    }

    var componentDescription: String {
        "\(depth) R\(red) G\(green) B\(blue) A\(alpha)"
    }
}
