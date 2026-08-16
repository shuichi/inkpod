import AppKit
import SwiftUI

struct WorkspaceChromeView: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @State private var measuredAvailableWidth: CGFloat = 0

    var body: some View {
        GeometryReader { geometry in
            chromeBody
                .preference(
                    key: WorkspaceAvailableWidthKey.self,
                    value: geometry.size.width
                )
        }
        .onPreferenceChange(WorkspaceAvailableWidthKey.self) { width in
            measuredAvailableWidth = width
        }
        .task(id: measuredAvailableWidth) {
            await Task.yield()
            publishAvailableWidth(measuredAvailableWidth)
        }
        .accessibilityIdentifier("inkpod.m11.workspace-chrome")
    }

    @ViewBuilder
    private var chromeBody: some View {
        if model.chromePreference.inspectorEdge == .trailing {
            HStack(spacing: 0) {
                toolSurface
                WorkspaceEditorContent(model: model, language: language)
            }
            .inspector(isPresented: inspectorBinding) {
                WorkspaceInspectorColumn(model: model, language: language)
                    .inspectorColumnWidth(
                        min: 240,
                        ideal: model.chromePreference.inspectorWidth,
                        max: 640
                    )
            }
        } else {
            HStack(spacing: 0) {
                if model.adaptiveChrome.inspectorVisible {
                    WorkspaceInspectorColumn(model: model, language: language)
                        .frame(width: model.chromePreference.inspectorWidth)
                        .background(LeadingInspectorGlassBackground())
                    Divider()
                }
                WorkspaceEditorContent(model: model, language: language)
                toolSurface
            }
        }
    }

    @ViewBuilder
    private var toolSurface: some View {
        if model.adaptiveChrome.toolPresentation != .hidden {
            if model.chromePreference.inspectorEdge == .leading {
                Divider()
            }
            GlassEffectContainer(spacing: 8) {
                M6ToolSidebar(
                    model: model,
                    language: language,
                    popoverArrowEdge: model.chromePreference.inspectorEdge == .trailing
                        ? .leading : .trailing
                )
                    .background {
                        if reduceTransparency {
                            RoundedRectangle(cornerRadius: 14)
                                .fill(Color(nsColor: .windowBackgroundColor))
                        }
                    }
                    .glassEffect(.regular, in: RoundedRectangle(cornerRadius: 14))
                    .accessibilityAction(
                        named: language.text("m11.tool.toggle.accessibility")
                    ) {
                        _ = model.reduceChrome(.toggleToolSurface)
                    }
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel(language.text("m11.tool.surface"))
            .accessibilityIdentifier("inkpod.m11.tool-surface")
            if model.chromePreference.inspectorEdge == .trailing {
                Divider()
            }
        }
    }

    private var inspectorBinding: Binding<Bool> {
        Binding(
            get: { model.adaptiveChrome.inspectorVisible },
            set: { model.setInspectorPresentedFromFramework($0) }
        )
    }

    private func publishAvailableWidth(_ contentWidth: CGFloat) {
        let nativeInspectorWidth = model.chromePreference.inspectorEdge == .trailing
            && model.adaptiveChrome.inspectorVisible
            ? model.chromePreference.inspectorWidth : 0
        let availableWidth = Double(contentWidth) + nativeInspectorWidth
        model.updateAdaptiveChrome(availableWidth: availableWidth)
    }
}

private struct WorkspaceInspectorColumn: View {
    @ObservedObject var model: WorkspaceModel
    @ObservedObject var language: AppLanguageController
    @State private var measuredWidth: CGFloat = 0

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 4) {
                ForEach(WorkspaceInspectorSection.allCases, id: \.self) { section in
                    Button {
                        model.selectInspectorSection(section)
                    } label: {
                        Image(systemName: symbol(for: section))
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderless)
                    .background(
                        model.chromePreference.selectedInspectorSection == section
                            ? Color.accentColor.opacity(0.2) : Color.clear,
                        in: RoundedRectangle(cornerRadius: 6)
                    )
                    .help(label(for: section))
                    .accessibilityLabel(label(for: section))
                    .accessibilityAddTraits(
                        model.chromePreference.selectedInspectorSection == section
                            ? .isSelected : []
                    )
                    .accessibilityIdentifier("inkpod.m11.inspector-tab.\(section.rawValue)")
                }
            }
            .padding(8)
            Divider()
            inspectorContent
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background {
            GeometryReader { geometry in
                Color.clear
                    .preference(
                        key: WorkspaceInspectorWidthKey.self,
                        value: geometry.size.width
                    )
            }
        }
        .onPreferenceChange(WorkspaceInspectorWidthKey.self) { width in
            measuredWidth = width
        }
        .task(id: measuredWidth) {
            await Task.yield()
            updateInspectorWidth(measuredWidth)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("inkpod.m11.inspector")
    }

    @ViewBuilder
    private var inspectorContent: some View {
        switch model.chromePreference.selectedInspectorSection {
        case .layerPlane:
            LayerPlaneInspector(model: model, language: language)
        case .color:
            M6ColorInspector(model: model, language: language)
        case .history:
            M7HistoryInspector(model: model, language: language)
        case .vectorAnnotationGuides:
            M8VectorAnnotationInspector(model: model, language: language)
        case .animation:
            M9AnimationInspector(model: model, language: language)
        }
    }

    private func updateInspectorWidth(_ width: CGFloat) {
        guard width.isFinite, width >= 240, width <= 640 else { return }
        let measuredWidth = Double(width)
        guard abs(model.chromePreference.inspectorWidth - measuredWidth) > 0.5 else { return }
        _ = model.reduceChrome(.setInspectorWidth(measuredWidth))
    }

    private func symbol(for section: WorkspaceInspectorSection) -> String {
        switch section {
        case .layerPlane: "square.3.layers.3d"
        case .color: "paintpalette"
        case .history: "clock.arrow.circlepath"
        case .vectorAnnotationGuides: "point.3.connected.trianglepath.dotted"
        case .animation: "film.stack"
        }
    }

    private func label(for section: WorkspaceInspectorSection) -> String {
        language.text("m11.inspector.tab.\(section.rawValue)")
    }
}

private struct WorkspaceAvailableWidthKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct WorkspaceInspectorWidthKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct LeadingInspectorGlassBackground: NSViewRepresentable {
    func makeNSView(context: Context) -> NSGlassEffectView {
        NSGlassEffectView()
    }

    func updateNSView(_ nsView: NSGlassEffectView, context: Context) {}
}
