import Foundation
import XCTest
@testable import InkpodCoreBridge

final class WorkspaceDomainTests: XCTestCase {
    func testEditorGraphLimitsSplitToTwoGroupsAndMovesViewsTransactionally() throws {
        let first = workspaceView(1, session: 1)
        let second = workspaceView(2, session: 2)
        var graph = WorkspaceEditorGraph(initialView: first)
        XCTAssertTrue(graph.appendAtomically([second], to: graph.activeGroupID))

        let splitView = workspaceView(3, session: 1)
        let other = try XCTUnwrap(graph.split(.vertical, with: splitView))
        XCTAssertEqual(graph.groups.count, 2)
        XCTAssertNil(graph.split(.horizontal, with: workspaceView(4, session: 1)))
        XCTAssertTrue(graph.move(viewID: second.id, to: other))
        XCTAssertEqual(graph.group(containing: second.id)?.id, other)
        XCTAssertFalse(graph.move(viewID: WorkspaceViewID(rawValue: 999), to: other))
    }

    func testDuplicateAndCrossWindowFailureRestoreOriginalGraphs() throws {
        let first = workspaceView(1, session: 1)
        let copied = workspaceView(2, session: 1)
        var source = WorkspaceEditorGraph(initialView: first)
        var destination = WorkspaceEditorGraph(initialView: workspaceView(3, session: 3))

        XCTAssertTrue(source.insertDuplicate(copied, after: first.id))
        XCTAssertEqual(source.allViews.map(\.session), [first.session, first.session])

        let originalSource = source
        let originalDestination = destination
        destination.maximumViewCount = 1
        XCTAssertFalse(
            WorkspaceEditorGraph.transfer(
                viewID: copied.id,
                from: &source,
                to: &destination,
                copy: false,
                copiedView: nil
            )
        )
        XCTAssertEqual(source, originalSource)
        XCTAssertEqual(destination.groups, originalDestination.groups)
    }

    func testCrossWindowMoveAndCopyPublishOnlyAfterDestinationAccepts() throws {
        let first = workspaceView(1, session: 1)
        let second = workspaceView(2, session: 1)
        var source = WorkspaceEditorGraph(initialView: first)
        XCTAssertTrue(source.appendAtomically([second], to: source.activeGroupID))
        var destination = WorkspaceEditorGraph(initialView: workspaceView(3, session: 3))

        XCTAssertTrue(
            WorkspaceEditorGraph.transfer(
                viewID: first.id,
                from: &source,
                to: &destination,
                copy: false,
                copiedView: nil
            )
        )
        XCTAssertFalse(source.allViews.contains { $0.id == first.id })
        XCTAssertEqual(destination.allViews.last, first)

        let duplicate = workspaceView(4, session: 1)
        let sourceBeforeCopy = source
        XCTAssertTrue(
            WorkspaceEditorGraph.transfer(
                viewID: second.id,
                from: &source,
                to: &destination,
                copy: true,
                copiedView: duplicate
            )
        )
        XCTAssertEqual(source, sourceBeforeCopy)
        XCTAssertEqual(destination.allViews.last, duplicate)
        XCTAssertNotEqual(duplicate.coreTarget, second.coreTarget)
        XCTAssertEqual(duplicate.session, second.session)
    }

    func testPanePinnedTargetNeverFallsBackToAnotherDocument() {
        let first = workspaceView(1, session: 1)
        let second = workspaceView(2, session: 2)
        var target = PaneTargetRecord.following
        XCTAssertEqual(target.resolve(active: first, liveViews: [first, second]), first)

        target.pin(to: first)
        XCTAssertEqual(target.resolve(active: second, liveViews: [first, second]), first)
        let sibling = workspaceView(3, session: 1)
        XCTAssertEqual(target.resolve(active: second, liveViews: [sibling, second]), sibling)
        XCTAssertNil(target.resolve(active: second, liveViews: [second]))
        XCTAssertEqual(target.mode, .followActiveView)
        XCTAssertNotNil(target.consumeAccessibilityNotice())
        XCTAssertEqual(target.resolve(active: second, liveViews: [second]), second)
    }

    func testWorkspaceRecordRoundTripsAndClampsToVisibleWorkArea() throws {
        let suite = "inkpod.workspace-domain.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = WorkspaceLayoutStore(defaults: defaults, key: "layout")
        let record = WorkspaceLayoutRecord(
            preset: .lineCleanup,
            split: .horizontal,
            splitRatio: 0.72,
            layerInspectorVisible: true,
            inspectorOnLeadingEdge: true,
            inspectorWidth: 420,
            layerPlaneRatio: 0.61,
            windowFrame: CGRect(x: 4_000, y: 4_000, width: 1_200, height: 900),
            customName: "Layout A"
        )
        XCTAssertTrue(store.save(record))
        let workArea = CGRect(x: 0, y: 0, width: 1_440, height: 900)
        let loaded = try XCTUnwrap(store.load(visibleWorkAreas: [workArea]))
        XCTAssertEqual(loaded.preset, .lineCleanup)
        XCTAssertEqual(loaded.customName, "Layout A")
        XCTAssertTrue(workArea.contains(loaded.windowFrame.origin))
        XCTAssertLessThanOrEqual(loaded.windowFrame.maxX, workArea.maxX)
        XCTAssertLessThanOrEqual(loaded.windowFrame.maxY, workArea.maxY)

        let secondary = CGRect(x: 1_920, y: 0, width: 1_920, height: 1_080)
        var secondaryRecord = record
        secondaryRecord.windowFrame = CGRect(x: 2_200, y: 80, width: 1_000, height: 720)
        XCTAssertTrue(store.save(secondaryRecord))
        let secondaryLoaded = try XCTUnwrap(
            store.load(visibleWorkAreas: [workArea, secondary])
        )
        XCTAssertGreaterThanOrEqual(secondaryLoaded.windowFrame.minX, secondary.minX)
        XCTAssertLessThanOrEqual(secondaryLoaded.windowFrame.maxX, secondary.maxX)

        defaults.set(Data([0xFF, 0x00]), forKey: "layout")
        let fallback = try XCTUnwrap(
            WorkspaceLayoutRecord.defaultColoring.validated(visibleWorkAreas: [workArea])
        )
        XCTAssertEqual(
            store.load(visibleWorkAreas: [workArea]),
            fallback
        )
    }

    func testChromeReducerOwnsToolInspectorSectionAndMirrorTransitions() {
        var preference = WorkspaceChromePreference.defaultColoring

        XCTAssertTrue(preference.reduce(.toggleToolSurface))
        XCTAssertEqual(preference.toolPresentation, .hidden)
        XCTAssertTrue(preference.reduce(.toggleToolOptions))
        XCTAssertEqual(preference.toolPresentation, .expanded)
        XCTAssertTrue(preference.reduce(.toggleToolOptions))
        XCTAssertEqual(preference.toolPresentation, .compact)

        XCTAssertTrue(preference.reduce(.toggleInspectorSection(.color)))
        XCTAssertTrue(preference.inspectorRequestedVisible)
        XCTAssertEqual(preference.selectedInspectorSection, .color)
        XCTAssertTrue(preference.reduce(.toggleInspectorSection(.color)))
        XCTAssertFalse(preference.inspectorRequestedVisible)
        XCTAssertEqual(preference.selectedInspectorSection, .color)
        XCTAssertFalse(preference.reduce(.setInspectorPresented(false)))

        XCTAssertTrue(preference.reduce(.showInspectorSection(.animation)))
        XCTAssertTrue(preference.inspectorRequestedVisible)
        XCTAssertEqual(preference.selectedInspectorSection, .animation)
        XCTAssertTrue(preference.reduce(.mirrorEdges))
        XCTAssertEqual(preference.inspectorEdge, .leading)
    }

    func testAdaptiveChromeProjectionIsOrderedReversibleAndNeverPersisted() {
        var preference = WorkspaceChromePreference.defaultColoring
        preference.toolPresentation = .expanded
        preference.inspectorWidth = 320

        let wide = AdaptiveChromeState.project(preference, availableWidth: 1_200)
        XCTAssertEqual(wide.toolPresentation, .expanded)
        XCTAssertTrue(wide.inspectorVisible)
        XCTAssertEqual(wide.canvasWidth, 702)

        let medium = AdaptiveChromeState.project(preference, availableWidth: 800)
        XCTAssertEqual(medium.toolPresentation, .compact)
        XCTAssertFalse(medium.inspectorVisible)
        XCTAssertEqual(medium.canvasWidth, 746)

        let narrow = AdaptiveChromeState.project(preference, availableWidth: 500)
        XCTAssertEqual(narrow.toolPresentation, .hidden)
        XCTAssertFalse(narrow.inspectorVisible)
        XCTAssertEqual(narrow.canvasWidth, 500)

        XCTAssertEqual(preference.toolPresentation, .expanded)
        XCTAssertTrue(preference.inspectorRequestedVisible)
        XCTAssertEqual(
            AdaptiveChromeState.project(preference, availableWidth: 1_200),
            wide
        )
        XCTAssertEqual(
            AdaptiveChromeState.project(preference, availableWidth: .nan),
            .project(preference, availableWidth: 1_200)
        )
    }

    func testWorkspaceLayoutMigratesV1AndRoundTripsExplicitV2Chrome() throws {
        let suite = "inkpod.workspace-m11.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = WorkspaceLayoutStore(defaults: defaults, key: "layout")
        let workArea = CGRect(x: 0, y: 0, width: 1_440, height: 900)
        let legacy = TestWorkspaceLayoutRecordV1(
            version: 1,
            preset: .lineCleanup,
            split: .horizontal,
            splitRatio: 0.72,
            layerInspectorVisible: true,
            inspectorOnLeadingEdge: true,
            inspectorWidth: 420,
            layerPlaneRatio: 0.61,
            windowFrame: CGRect(x: 120, y: 120, width: 1_200, height: 800),
            customName: "Legacy"
        )
        let legacyData = try PropertyListEncoder().encode(legacy)
        defaults.set(legacyData, forKey: "layout")

        let migrated = try XCTUnwrap(store.load(visibleWorkAreas: [workArea]))
        XCTAssertEqual(migrated.version, 2)
        XCTAssertEqual(migrated.chrome.toolPresentation, .expanded)
        XCTAssertTrue(migrated.chrome.inspectorRequestedVisible)
        XCTAssertEqual(migrated.chrome.selectedInspectorSection, .layerPlane)
        XCTAssertEqual(migrated.chrome.inspectorEdge, .leading)
        XCTAssertEqual(migrated.chrome.inspectorWidth, 420)

        var explicit = migrated
        explicit.chrome.toolPresentation = .compact
        explicit.chrome.selectedInspectorSection = .history
        explicit.chrome.inspectorRequestedVisible = false
        XCTAssertTrue(store.save(explicit))
        XCTAssertEqual(store.load(visibleWorkAreas: [workArea]), explicit)

        var malformed = legacy
        malformed.version = 99
        defaults.set(try PropertyListEncoder().encode(malformed), forKey: "layout")
        let fallback = try XCTUnwrap(
            WorkspaceLayoutRecord.defaultColoring.validated(visibleWorkAreas: [workArea])
        )
        XCTAssertEqual(store.load(visibleWorkAreas: [workArea]), fallback)
    }

    func testWorkspaceLayoutRejectsEveryInvalidV2ChromeField() throws {
        let suite = "inkpod.workspace-m11-invalid.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = WorkspaceLayoutStore(defaults: defaults, key: "layout")
        let workArea = CGRect(x: 0, y: 0, width: 1_440, height: 900)
        let fallback = try XCTUnwrap(
            WorkspaceLayoutRecord.defaultColoring.validated(visibleWorkAreas: [workArea])
        )
        let validData = try PropertyListEncoder().encode(WorkspaceLayoutRecord.defaultColoring)
        let validObject = try XCTUnwrap(
            try PropertyListSerialization.propertyList(
                from: validData,
                options: .mutableContainersAndLeaves,
                format: nil
            ) as? NSMutableDictionary
        )

        for (field, invalidValue) in [
            ("toolPresentation", "wide" as Any),
            ("selectedInspectorSection", "metadata" as Any),
            ("inspectorEdge", "center" as Any),
            ("inspectorWidth", 239 as Any),
        ] {
            let candidate = validObject.mutableCopy() as! NSMutableDictionary
            let chrome = try XCTUnwrap(
                (validObject["chrome"] as? NSDictionary)?.mutableCopy()
                    as? NSMutableDictionary
            )
            chrome[field] = invalidValue
            candidate["chrome"] = chrome
            defaults.set(
                try PropertyListSerialization.data(
                    fromPropertyList: candidate,
                    format: .binary,
                    options: 0
                ),
                forKey: "layout"
            )
            XCTAssertEqual(
                store.load(visibleWorkAreas: [workArea]),
                fallback,
                "invalid \(field) must fall back atomically"
            )
        }
    }

    func testAdaptiveProjectionIsNotWrittenIntoWorkspaceRecord() {
        var record = WorkspaceLayoutRecord.defaultColoring
        record.chrome.toolPresentation = .expanded
        record.chrome.inspectorRequestedVisible = true
        let projected = AdaptiveChromeState.project(record.chrome, availableWidth: 640)

        XCTAssertEqual(projected.toolPresentation, .compact)
        XCTAssertFalse(projected.inspectorVisible)
        XCTAssertEqual(record.chrome.toolPresentation, .expanded)
        XCTAssertTrue(record.chrome.inspectorRequestedVisible)
    }
}

private struct TestWorkspaceLayoutRecordV1: Codable {
    var version: UInt32
    var preset: WorkspacePreset
    var split: WorkspaceSplitOrientation?
    var splitRatio: Double
    var layerInspectorVisible: Bool
    var inspectorOnLeadingEdge: Bool
    var inspectorWidth: Double
    var layerPlaneRatio: Double
    var windowFrame: CGRect
    var customName: String?
}

private func workspaceView(_ value: UInt64, session: UInt64) -> WorkspaceViewRecord {
    let sessionTarget = CoreSessionTarget(
        id: CoreSessionID(rawValue: session),
        generation: CoreSessionGeneration(rawValue: session)
    )
    return WorkspaceViewRecord(
        id: WorkspaceViewID(rawValue: value),
        coreTarget: CoreViewTarget(
            session: sessionTarget,
            id: CoreViewID(rawValue: value),
            generation: CoreViewGeneration(rawValue: value)
        ),
        session: sessionTarget,
        viewRevision: 1,
        title: "View \(value)"
    )
}
