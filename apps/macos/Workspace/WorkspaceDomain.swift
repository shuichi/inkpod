import CoreGraphics
import Foundation

public struct NewCellDraft: Identifiable, Equatable, Sendable {
    public let id = UUID()
    public var sizingMode: CoreCellSizingMode = .imagePixels
    public var width: UInt32 = 1_920
    public var height: UInt32 = 1_080
    public var dpiXMilli: UInt32 = 72_000
    public var dpiYMilli: UInt32 = 72_000
    public var marginMilli: UInt32 = 0
    public var safeFrameRatioMilli: UInt32 = 900
    public var maximumCloseRatioMilli: UInt32 = 1_000
    public var anchor: CoreFrameAnchor = .center
    public var initialLayerKind: CoreLayerKind = .binaryColoring
    public var pixelFormat: CorePixelStorageFormat = .rgba8
    public var count: UInt32 = 1

    public init() {}

    public var options: CoreCellCreationOptions {
        CoreCellCreationOptions(
            sizingMode: sizingMode,
            width: width,
            height: height,
            dpiXMilli: dpiXMilli,
            dpiYMilli: dpiYMilli,
            marginMilli: marginMilli,
            safeFrameRatioMilli: safeFrameRatioMilli,
            maximumCloseRatioMilli: maximumCloseRatioMilli,
            anchor: anchor,
            initialLayerKind: initialLayerKind,
            pixelFormat: pixelFormat,
            count: count
        )
    }
}

public struct CellEditorDraft: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case paperFrames
        case resize(allowsResample: Bool)
    }

    public let id = UUID()
    public let kind: Kind
    public var frames: CorePaperFrames
    public var width: UInt32
    public var height: UInt32
    public var dpiXMilli: UInt32
    public var dpiYMilli: UInt32
    public var anchor: CoreFrameAnchor
    public var resample: Bool

    public init(kind: Kind, session: CoreSessionProjection) {
        self.kind = kind
        frames = session.paperFrames
        width = session.documentWidth
        height = session.documentHeight
        dpiXMilli = session.dpiXMilli
        dpiYMilli = session.dpiYMilli
        anchor = .center
        resample = false
    }

    public var command: CoreCellEditCommand {
        switch kind {
        case .paperFrames:
            .updatePaperFrames(frames)
        case .resize:
            .resize(CoreDocumentResize(
                width: width,
                height: height,
                dpiXMilli: dpiXMilli,
                dpiYMilli: dpiYMilli,
                anchor: anchor,
                resample: resample
            ))
        }
    }
}

public struct TreeEditorDraft: Identifiable, Equatable, Sendable {
    public enum Kind: Equatable, Sendable {
        case createLayer
        case createPlane(parentLayerID: UInt64)
        case layerProperties(CoreLayerProjection)
        case planeProperties(CoreNodeProjection)
        case convertLayer(id: UInt64)
        case convertPlane(id: UInt64, parentLayerID: UInt64)
    }

    public let id = UUID()
    public let kind: Kind
    public var name: String
    public var layerKind: CoreLayerKind
    public var planeKind: CorePlaneKind
    public var pixelFormat: CorePixelStorageFormat
    public var opacityMilli: UInt32
    public var visible: Bool
    public var editable: Bool

    public init(kind: Kind, defaultName: String? = nil) {
        self.kind = kind
        switch kind {
        case .createLayer:
            name = defaultName ?? "Layer"
            layerKind = .raster
            planeKind = .raster
            pixelFormat = .rgba8
            opacityMilli = 1_000
            visible = true
            editable = true
        case .createPlane:
            name = defaultName ?? "Plane"
            layerKind = .raster
            planeKind = .raster
            pixelFormat = .rgba8
            opacityMilli = 1_000
            visible = true
            editable = true
        case let .layerProperties(layer):
            name = layer.name
            layerKind = layer.kind
            planeKind = .raster
            pixelFormat = layer.pixelFormat == .none ? .rgba8 : layer.pixelFormat
            opacityMilli = layer.opacityMilli
            visible = layer.isVisible
            editable = layer.isEditable
        case let .planeProperties(plane):
            name = plane.name
            layerKind = .raster
            planeKind = plane.kind
            pixelFormat = plane.pixelFormat
            opacityMilli = plane.opacityMilli
            visible = plane.isVisible
            editable = plane.isEditable
        case .convertLayer:
            name = ""
            layerKind = .raster
            planeKind = .raster
            pixelFormat = .rgba8
            opacityMilli = 1_000
            visible = true
            editable = true
        case .convertPlane:
            name = ""
            layerKind = .raster
            planeKind = .raster
            pixelFormat = .rgba8
            opacityMilli = 1_000
            visible = true
            editable = true
        }
    }

    public var command: CoreTreeEditCommand {
        switch kind {
        case .createLayer:
            .createLayer(kind: layerKind, pixelFormat: pixelFormat, name: name)
        case let .createPlane(parent):
            .createPlane(
                parentLayerID: parent,
                kind: planeKind,
                pixelFormat: pixelFormat,
                name: name
            )
        case let .layerProperties(layer):
            .setLayerProperties(
                id: layer.id,
                visible: visible,
                editable: editable,
                opacityMilli: opacityMilli,
                name: name
            )
        case let .planeProperties(plane):
            .setPlaneProperties(
                id: plane.id,
                parentLayerID: plane.parentID,
                visible: visible,
                editable: editable,
                opacityMilli: opacityMilli,
                name: name
            )
        case let .convertLayer(id):
            .convertLayer(id: id, kind: layerKind, pixelFormat: pixelFormat)
        case let .convertPlane(id, parent):
            .convertPlane(
                id: id,
                parentLayerID: parent,
                kind: planeKind,
                pixelFormat: pixelFormat
            )
        }
    }
}

public struct WorkspaceViewID: RawRepresentable, Hashable, Codable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct EditorGroupID: RawRepresentable, Hashable, Codable, Sendable {
    public let rawValue: UInt64

    public init(rawValue: UInt64) {
        self.rawValue = rawValue
    }
}

public struct WorkspaceViewRecord: Equatable, Identifiable, Sendable {
    public let id: WorkspaceViewID
    public let coreTarget: CoreViewTarget
    public let session: CoreSessionTarget
    public var viewRevision: UInt64
    public var title: String

    public init(
        id: WorkspaceViewID,
        coreTarget: CoreViewTarget,
        session: CoreSessionTarget,
        viewRevision: UInt64,
        title: String
    ) {
        self.id = id
        self.coreTarget = coreTarget
        self.session = session
        self.viewRevision = viewRevision
        self.title = title
    }
}

struct WorkspaceViewTransfer: Sendable {
    let view: WorkspaceViewRecord
    let session: CoreSessionProjection
    let removesSource: Bool
}

public struct EditorGroupRecord: Equatable, Identifiable, Sendable {
    public let id: EditorGroupID
    public var views: [WorkspaceViewRecord]
    public var activeViewID: WorkspaceViewID

    public init(id: EditorGroupID, views: [WorkspaceViewRecord], activeViewID: WorkspaceViewID) {
        precondition(!views.isEmpty && views.contains { $0.id == activeViewID })
        self.id = id
        self.views = views
        self.activeViewID = activeViewID
    }

    public var activeView: WorkspaceViewRecord? {
        views.first { $0.id == activeViewID }
    }
}

public enum WorkspaceSplitOrientation: String, Codable, CaseIterable, Sendable {
    case horizontal
    case vertical
}

public struct WorkspaceEditorGraph: Equatable, Sendable {
    public private(set) var groups: [EditorGroupRecord]
    public private(set) var activeGroupID: EditorGroupID
    public private(set) var splitOrientation: WorkspaceSplitOrientation?
    public var maximumViewCount = 64
    private var nextGroupID: UInt64

    public init(initialView: WorkspaceViewRecord) {
        let group = EditorGroupRecord(
            id: EditorGroupID(rawValue: 1),
            views: [initialView],
            activeViewID: initialView.id
        )
        groups = [group]
        activeGroupID = group.id
        splitOrientation = nil
        nextGroupID = 2
    }

    public var allViews: [WorkspaceViewRecord] { groups.flatMap(\.views) }
    public var activeView: WorkspaceViewRecord? { group(id: activeGroupID)?.activeView }

    public func group(id: EditorGroupID) -> EditorGroupRecord? {
        groups.first { $0.id == id }
    }

    public func group(containing viewID: WorkspaceViewID) -> EditorGroupRecord? {
        groups.first { group in group.views.contains { $0.id == viewID } }
    }

    @discardableResult
    public mutating func updateViewRevision(
        target: CoreViewTarget,
        revision: UInt64
    ) -> Bool {
        for groupIndex in groups.indices {
            guard let viewIndex = groups[groupIndex].views.firstIndex(where: {
                $0.coreTarget == target
            }) else { continue }
            groups[groupIndex].views[viewIndex].viewRevision = revision
            return true
        }
        return false
    }

    public mutating func activate(groupID: EditorGroupID, viewID: WorkspaceViewID? = nil) -> Bool {
        guard let groupIndex = groups.firstIndex(where: { $0.id == groupID }) else { return false }
        if let viewID {
            guard groups[groupIndex].views.contains(where: { $0.id == viewID }) else { return false }
            groups[groupIndex].activeViewID = viewID
        }
        activeGroupID = groupID
        return true
    }

    @discardableResult
    public mutating func appendAtomically(
        _ views: [WorkspaceViewRecord],
        to groupID: EditorGroupID
    ) -> Bool {
        guard !views.isEmpty,
              allViews.count + views.count <= maximumViewCount,
              Set(views.map(\.id)).count == views.count,
              Set(allViews.map(\.id)).isDisjoint(with: views.map(\.id)),
              let index = groups.firstIndex(where: { $0.id == groupID })
        else {
            return false
        }
        groups[index].views.append(contentsOf: views)
        groups[index].activeViewID = views.last!.id
        activeGroupID = groupID
        return true
    }

    @discardableResult
    public mutating func split(
        _ orientation: WorkspaceSplitOrientation,
        with view: WorkspaceViewRecord
    ) -> EditorGroupID? {
        guard groups.count == 1, nextGroupID != 0, nextGroupID < UInt64.max,
              allViews.count < maximumViewCount,
              !allViews.contains(where: { $0.id == view.id })
        else {
            return nil
        }
        let id = EditorGroupID(rawValue: nextGroupID)
        nextGroupID += 1
        groups.append(EditorGroupRecord(id: id, views: [view], activeViewID: view.id))
        splitOrientation = orientation
        activeGroupID = id
        return id
    }

    @discardableResult
    public mutating func closeGroup(_ id: EditorGroupID) -> [WorkspaceViewRecord]? {
        guard groups.count == 2,
              let removedIndex = groups.firstIndex(where: { $0.id == id })
        else {
            return nil
        }
        let removed = groups.remove(at: removedIndex)
        let existingIDs = Set(groups[0].views.map(\.id))
        let moved = removed.views.filter { !existingIDs.contains($0.id) }
        groups[0].views.append(contentsOf: moved)
        activeGroupID = groups[0].id
        splitOrientation = nil
        return moved
    }

    @discardableResult
    public mutating func move(viewID: WorkspaceViewID, to destination: EditorGroupID) -> Bool {
        guard let sourceIndex = groups.firstIndex(where: { group in
            group.views.contains { $0.id == viewID }
        }),
        let destinationIndex = groups.firstIndex(where: { $0.id == destination }),
        sourceIndex != destinationIndex,
        !groups[destinationIndex].views.contains(where: { $0.id == viewID })
        else {
            return false
        }
        let viewIndex = groups[sourceIndex].views.firstIndex { $0.id == viewID }!
        let view = groups[sourceIndex].views.remove(at: viewIndex)
        if groups[sourceIndex].views.isEmpty {
            groups.remove(at: sourceIndex)
            let adjustedDestination = groups.firstIndex(where: { $0.id == destination })!
            groups[adjustedDestination].views.append(view)
            groups[adjustedDestination].activeViewID = viewID
            activeGroupID = destination
            splitOrientation = nil
            return true
        } else if groups[sourceIndex].activeViewID == viewID {
            groups[sourceIndex].activeViewID = groups[sourceIndex].views[0].id
        }
        groups[destinationIndex].views.append(view)
        groups[destinationIndex].activeViewID = viewID
        activeGroupID = destination
        return true
    }

    @discardableResult
    public mutating func cycleTab(in groupID: EditorGroupID, delta: Int) -> Bool {
        guard delta != 0,
              let index = groups.firstIndex(where: { $0.id == groupID }),
              groups[index].views.count > 1,
              let current = groups[index].views.firstIndex(where: {
                  $0.id == groups[index].activeViewID
              })
        else {
            return false
        }
        let count = groups[index].views.count
        groups[index].activeViewID = groups[index].views[(current + delta + count) % count].id
        activeGroupID = groupID
        return true
    }

    @discardableResult
    public mutating func reorderTab(viewID: WorkspaceViewID, delta: Int) -> Bool {
        guard delta != 0,
              let groupIndex = groups.firstIndex(where: { group in
                  group.views.contains { $0.id == viewID }
              }),
              let source = groups[groupIndex].views.firstIndex(where: { $0.id == viewID })
        else {
            return false
        }
        let destination = source + delta
        guard groups[groupIndex].views.indices.contains(destination) else { return false }
        groups[groupIndex].views.swapAt(source, destination)
        return true
    }

    @discardableResult
    public mutating func insertDuplicate(
        _ duplicate: WorkspaceViewRecord,
        after sourceID: WorkspaceViewID
    ) -> Bool {
        guard allViews.count < maximumViewCount,
              !allViews.contains(where: { $0.id == duplicate.id }),
              let groupIndex = groups.firstIndex(where: { group in
                  group.views.contains { $0.id == sourceID }
              }),
              let sourceIndex = groups[groupIndex].views.firstIndex(where: { $0.id == sourceID })
        else {
            return false
        }
        groups[groupIndex].views.insert(duplicate, at: sourceIndex + 1)
        groups[groupIndex].activeViewID = duplicate.id
        activeGroupID = groups[groupIndex].id
        return true
    }

    @discardableResult
    public mutating func remove(viewID: WorkspaceViewID) -> WorkspaceViewRecord? {
        guard let groupIndex = groups.firstIndex(where: { group in
            group.views.contains { $0.id == viewID }
        }),
        groups[groupIndex].views.count > 1 || groups.count > 1,
        let viewIndex = groups[groupIndex].views.firstIndex(where: { $0.id == viewID })
        else {
            return nil
        }
        let removed = groups[groupIndex].views.remove(at: viewIndex)
        if groups[groupIndex].views.isEmpty {
            groups.remove(at: groupIndex)
            splitOrientation = nil
            activeGroupID = groups[0].id
        } else if groups[groupIndex].activeViewID == viewID {
            groups[groupIndex].activeViewID = groups[groupIndex].views[0].id
        }
        return removed
    }

    @discardableResult
    public mutating func extractForWindowTransfer(
        viewID: WorkspaceViewID
    ) -> WorkspaceViewRecord? {
        guard let groupIndex = groups.firstIndex(where: { group in
            group.views.contains { $0.id == viewID }
        }),
        let viewIndex = groups[groupIndex].views.firstIndex(where: { $0.id == viewID })
        else {
            return nil
        }
        let removed = groups[groupIndex].views.remove(at: viewIndex)
        if groups[groupIndex].views.isEmpty {
            groups.remove(at: groupIndex)
            splitOrientation = nil
            activeGroupID = groups.first?.id ?? EditorGroupID(rawValue: 0)
        } else if groups[groupIndex].activeViewID == viewID {
            groups[groupIndex].activeViewID = groups[groupIndex].views[0].id
        }
        return removed
    }

    public static func transfer(
        viewID: WorkspaceViewID,
        from source: inout WorkspaceEditorGraph,
        to destination: inout WorkspaceEditorGraph,
        copy: Bool,
        copiedView: WorkspaceViewRecord?
    ) -> Bool {
        let originalSource = source
        let originalDestination = destination
        guard let sourceView = source.allViews.first(where: { $0.id == viewID }) else {
            return false
        }
        let transferred: WorkspaceViewRecord
        if copy {
            guard let copiedView else { return false }
            transferred = copiedView
        } else {
            transferred = sourceView
            guard source.extractForWindowTransfer(viewID: viewID) != nil else { return false }
        }
        guard destination.appendAtomically([transferred], to: destination.activeGroupID) else {
            source = originalSource
            destination = originalDestination
            return false
        }
        return true
    }
}

public enum PaneTargetMode: Equatable, Sendable {
    case followActiveView
    case pinnedDocument(CoreSessionTarget)
}

public enum PaneTargetNotice: Equatable, Sendable {
    case pinnedDocumentClosed
}

public struct PaneTargetRecord: Equatable, Sendable {
    public private(set) var mode: PaneTargetMode
    private var pinnedView: WorkspaceViewRecord?
    private var accessibilityNotice: PaneTargetNotice?

    public static let following = PaneTargetRecord(
        mode: .followActiveView,
        pinnedView: nil,
        accessibilityNotice: nil
    )

    public mutating func pin(to view: WorkspaceViewRecord) {
        mode = .pinnedDocument(view.session)
        pinnedView = view
        accessibilityNotice = nil
    }

    public mutating func follow() {
        mode = .followActiveView
        pinnedView = nil
    }

    public mutating func resolve(
        active: WorkspaceViewRecord?,
        liveViews: [WorkspaceViewRecord]
    ) -> WorkspaceViewRecord? {
        switch mode {
        case .followActiveView:
            return active
        case let .pinnedDocument(session):
            let exact = pinnedView.flatMap { pinned in
                liveViews.first(where: { $0.id == pinned.id && $0.session == session })
            }
            guard let resolved = exact ?? liveViews.first(where: { $0.session == session }) else {
                mode = .followActiveView
                self.pinnedView = nil
                accessibilityNotice = .pinnedDocumentClosed
                return nil
            }
            if pinnedView?.id != resolved.id {
                pinnedView = resolved
            }
            return pinnedView
        }
    }

    public mutating func consumeAccessibilityNotice() -> PaneTargetNotice? {
        defer { accessibilityNotice = nil }
        return accessibilityNotice
    }
}

public enum WorkspacePreset: String, CaseIterable, Codable, Sendable {
    case coloring
    case lineCleanup
    case referenceCheck
    case batch
    case focus
}

public enum WorkspaceToolPresentation: String, CaseIterable, Codable, Sendable {
    case hidden
    case compact
    case expanded

    public var width: Double {
        switch self {
        case .hidden: 0
        case .compact: 54
        case .expanded: 178
        }
    }
}

public enum WorkspaceInspectorSection: String, CaseIterable, Codable, Sendable {
    case layerPlane
    case color
    case history
    case vectorAnnotationGuides
    case animation
}

public enum WorkspaceInspectorEdge: String, CaseIterable, Codable, Sendable {
    case leading
    case trailing
}

public enum WorkspaceChromeAction: Equatable, Sendable {
    case toggleToolSurface
    case toggleToolOptions
    case setToolPresentation(WorkspaceToolPresentation)
    case toggleInspector
    case setInspectorPresented(Bool)
    case selectInspectorSection(WorkspaceInspectorSection)
    case showInspectorSection(WorkspaceInspectorSection)
    case toggleInspectorSection(WorkspaceInspectorSection)
    case mirrorEdges
    case setInspectorWidth(Double)
    case restore(WorkspaceChromePreference)
}

public struct WorkspaceChromePreference: Equatable, Codable, Sendable {
    public var toolPresentation: WorkspaceToolPresentation
    public var inspectorRequestedVisible: Bool
    public var selectedInspectorSection: WorkspaceInspectorSection
    public var inspectorEdge: WorkspaceInspectorEdge
    public var inspectorWidth: Double

    public init(
        toolPresentation: WorkspaceToolPresentation,
        inspectorRequestedVisible: Bool,
        selectedInspectorSection: WorkspaceInspectorSection,
        inspectorEdge: WorkspaceInspectorEdge,
        inspectorWidth: Double
    ) {
        self.toolPresentation = toolPresentation
        self.inspectorRequestedVisible = inspectorRequestedVisible
        self.selectedInspectorSection = selectedInspectorSection
        self.inspectorEdge = inspectorEdge
        self.inspectorWidth = inspectorWidth
    }

    public static let defaultColoring = WorkspaceChromePreference(
        toolPresentation: .expanded,
        inspectorRequestedVisible: true,
        selectedInspectorSection: .layerPlane,
        inspectorEdge: .trailing,
        inspectorWidth: 320
    )

    @discardableResult
    public mutating func reduce(_ action: WorkspaceChromeAction) -> Bool {
        let previous = self
        switch action {
        case .toggleToolSurface:
            toolPresentation = toolPresentation == .hidden ? .compact : .hidden
        case .toggleToolOptions:
            toolPresentation = switch toolPresentation {
            case .hidden, .compact: .expanded
            case .expanded: .compact
            }
        case let .setToolPresentation(presentation):
            toolPresentation = presentation
        case .toggleInspector:
            inspectorRequestedVisible.toggle()
        case let .setInspectorPresented(isPresented):
            inspectorRequestedVisible = isPresented
        case let .selectInspectorSection(section):
            selectedInspectorSection = section
        case let .showInspectorSection(section):
            selectedInspectorSection = section
            inspectorRequestedVisible = true
        case let .toggleInspectorSection(section):
            if inspectorRequestedVisible, selectedInspectorSection == section {
                inspectorRequestedVisible = false
            } else {
                selectedInspectorSection = section
                inspectorRequestedVisible = true
            }
        case .mirrorEdges:
            inspectorEdge = inspectorEdge == .leading ? .trailing : .leading
        case let .setInspectorWidth(width):
            guard width.isFinite, (240 ... 640).contains(width) else { return false }
            inspectorWidth = width
        case let .restore(preference):
            guard preference.isValid else { return false }
            self = preference
        }
        return self != previous
    }

    fileprivate var isValid: Bool {
        inspectorWidth.isFinite && (240 ... 640).contains(inspectorWidth)
    }
}

public struct AdaptiveChromeState: Equatable, Sendable {
    public static let minimumCanvasWidth = 480.0

    public let toolPresentation: WorkspaceToolPresentation
    public let inspectorVisible: Bool
    public let canvasWidth: Double

    public static func project(
        _ preference: WorkspaceChromePreference,
        availableWidth: Double
    ) -> AdaptiveChromeState {
        let width = availableWidth.isFinite && availableWidth > 0 ? availableWidth : 1_200
        var tool = preference.toolPresentation
        var inspectorVisible = preference.inspectorRequestedVisible

        func projectedCanvasWidth() -> Double {
            max(0, width - tool.width - (inspectorVisible ? preference.inspectorWidth : 0))
        }

        if projectedCanvasWidth() < minimumCanvasWidth, tool == .expanded {
            tool = .compact
        }
        if projectedCanvasWidth() < minimumCanvasWidth, inspectorVisible {
            inspectorVisible = false
        }
        if projectedCanvasWidth() < minimumCanvasWidth, tool != .hidden {
            tool = .hidden
        }
        return AdaptiveChromeState(
            toolPresentation: tool,
            inspectorVisible: inspectorVisible,
            canvasWidth: projectedCanvasWidth()
        )
    }
}

public struct WorkspaceLayoutRecord: Equatable, Codable, Sendable {
    private static let currentVersion: UInt32 = 2

    public let version: UInt32
    public var preset: WorkspacePreset
    public var split: WorkspaceSplitOrientation?
    public var splitRatio: Double
    public var chrome: WorkspaceChromePreference
    public var layerPlaneRatio: Double
    public var windowFrame: CGRect
    public var customName: String?

    public init(
        preset: WorkspacePreset,
        split: WorkspaceSplitOrientation?,
        splitRatio: Double,
        layerInspectorVisible: Bool,
        inspectorOnLeadingEdge: Bool,
        inspectorWidth: Double,
        layerPlaneRatio: Double,
        windowFrame: CGRect,
        customName: String?
    ) {
        version = Self.currentVersion
        self.preset = preset
        self.split = split
        self.splitRatio = splitRatio
        chrome = WorkspaceChromePreference(
            toolPresentation: .expanded,
            inspectorRequestedVisible: layerInspectorVisible,
            selectedInspectorSection: .layerPlane,
            inspectorEdge: inspectorOnLeadingEdge ? .leading : .trailing,
            inspectorWidth: inspectorWidth
        )
        self.layerPlaneRatio = layerPlaneRatio
        self.windowFrame = windowFrame
        self.customName = customName
    }

    public init(
        preset: WorkspacePreset,
        split: WorkspaceSplitOrientation?,
        splitRatio: Double,
        chrome: WorkspaceChromePreference,
        layerPlaneRatio: Double,
        windowFrame: CGRect,
        customName: String?
    ) {
        version = Self.currentVersion
        self.preset = preset
        self.split = split
        self.splitRatio = splitRatio
        self.chrome = chrome
        self.layerPlaneRatio = layerPlaneRatio
        self.windowFrame = windowFrame
        self.customName = customName
    }

    public var layerInspectorVisible: Bool {
        get { chrome.inspectorRequestedVisible }
        set { chrome.inspectorRequestedVisible = newValue }
    }

    public var inspectorOnLeadingEdge: Bool {
        get { chrome.inspectorEdge == .leading }
        set { chrome.inspectorEdge = newValue ? .leading : .trailing }
    }

    public var inspectorWidth: Double {
        get { chrome.inspectorWidth }
        set { chrome.inspectorWidth = newValue }
    }

    public static let defaultColoring = WorkspaceLayoutRecord(
        preset: .coloring,
        split: nil,
        splitRatio: 0.5,
        layerInspectorVisible: true,
        inspectorOnLeadingEdge: false,
        inspectorWidth: 320,
        layerPlaneRatio: 0.55,
        windowFrame: CGRect(x: 120, y: 120, width: 1_200, height: 800),
        customName: nil
    )

    func validated(visibleWorkAreas: [CGRect]) -> WorkspaceLayoutRecord? {
        guard version == Self.currentVersion,
              splitRatio.isFinite, (0.2 ... 0.8).contains(splitRatio),
              chrome.isValid,
              layerPlaneRatio.isFinite, (0.2 ... 0.8).contains(layerPlaneRatio),
              windowFrame.width.isFinite, windowFrame.height.isFinite,
              (640 ... 16_384).contains(windowFrame.width),
              (480 ... 16_384).contains(windowFrame.height),
              customName?.utf8.count ?? 0 <= 256,
              let workArea = Self.bestWorkArea(for: windowFrame, in: visibleWorkAreas)
        else {
            return nil
        }
        var result = self
        let width = min(windowFrame.width, workArea.width)
        let height = min(windowFrame.height, workArea.height)
        let x = min(max(windowFrame.minX, workArea.minX), workArea.maxX - width)
        let y = min(max(windowFrame.minY, workArea.minY), workArea.maxY - height)
        result.windowFrame = CGRect(x: x, y: y, width: width, height: height)
        return result
    }

    private static func bestWorkArea(for frame: CGRect, in workAreas: [CGRect]) -> CGRect? {
        guard !workAreas.isEmpty else { return nil }
        return workAreas.max { lhs, rhs in
            let lhsIntersection = intersectionArea(frame, lhs)
            let rhsIntersection = intersectionArea(frame, rhs)
            if lhsIntersection != rhsIntersection {
                return lhsIntersection < rhsIntersection
            }
            return squaredDistance(frame.center, lhs.center)
                > squaredDistance(frame.center, rhs.center)
        }
    }

    private static func intersectionArea(_ lhs: CGRect, _ rhs: CGRect) -> CGFloat {
        let intersection = lhs.intersection(rhs)
        return intersection.isNull ? 0 : intersection.width * intersection.height
    }

    private static func squaredDistance(_ lhs: CGPoint, _ rhs: CGPoint) -> CGFloat {
        let dx = lhs.x - rhs.x
        let dy = lhs.y - rhs.y
        return dx * dx + dy * dy
    }
}

private struct WorkspaceLayoutRecordV1: Codable {
    let version: UInt32
    var preset: WorkspacePreset
    var split: WorkspaceSplitOrientation?
    var splitRatio: Double
    var layerInspectorVisible: Bool
    var inspectorOnLeadingEdge: Bool
    var inspectorWidth: Double
    var layerPlaneRatio: Double
    var windowFrame: CGRect
    var customName: String?

    var migrated: WorkspaceLayoutRecord {
        WorkspaceLayoutRecord(
            preset: preset,
            split: split,
            splitRatio: splitRatio,
            chrome: WorkspaceChromePreference(
                toolPresentation: .expanded,
                inspectorRequestedVisible: layerInspectorVisible,
                selectedInspectorSection: .layerPlane,
                inspectorEdge: inspectorOnLeadingEdge ? .leading : .trailing,
                inspectorWidth: inspectorWidth
            ),
            layerPlaneRatio: layerPlaneRatio,
            windowFrame: windowFrame,
            customName: customName
        )
    }
}

private extension CGRect {
    var center: CGPoint { CGPoint(x: midX, y: midY) }
}

public final class WorkspaceLayoutStore: @unchecked Sendable {
    private let defaults: UserDefaults
    private let key: String

    public init(defaults: UserDefaults = .standard, key: String = "workspace-layout-v1") {
        self.defaults = defaults
        self.key = key
    }

    @discardableResult
    public func save(_ record: WorkspaceLayoutRecord) -> Bool {
        guard let data = try? PropertyListEncoder().encode(record), data.count <= 64 * 1_024 else {
            return false
        }
        defaults.set(data, forKey: key)
        return true
    }

    public func load(visibleWorkAreas: [CGRect]) -> WorkspaceLayoutRecord? {
        guard let data = defaults.data(forKey: key) else { return nil }
        guard data.count <= 64 * 1_024,
              let propertyList = try? PropertyListSerialization.propertyList(
                  from: data,
                  options: [],
                  format: nil
              ),
              let dictionary = propertyList as? [String: Any],
              let number = dictionary["version"] as? NSNumber
        else {
            return fallback(visibleWorkAreas: visibleWorkAreas)
        }
        let decoder = PropertyListDecoder()
        let decoded: WorkspaceLayoutRecord?
        switch number.uint32Value {
        case 1:
            decoded = try? decoder.decode(WorkspaceLayoutRecordV1.self, from: data).migrated
        case 2:
            decoded = try? decoder.decode(WorkspaceLayoutRecord.self, from: data)
        default:
            decoded = nil
        }
        return decoded?.validated(visibleWorkAreas: visibleWorkAreas)
            ?? fallback(visibleWorkAreas: visibleWorkAreas)
    }

    private func fallback(visibleWorkAreas: [CGRect]) -> WorkspaceLayoutRecord {
        WorkspaceLayoutRecord.defaultColoring.validated(visibleWorkAreas: visibleWorkAreas)
            ?? .defaultColoring
    }
}
