import AppKit
import InkpodCoreBridge
import SwiftUI

@main
struct InkpodApp: App {
    @NSApplicationDelegateAdaptor(InkpodAppDelegate.self) private var appDelegate
    @StateObject private var application = ApplicationCoordinator()

    var body: some Scene {
        WindowGroup(for: WorkspaceID.self) { $workspaceID in
            InkpodWorkspaceScene(
                id: workspaceID,
                application: application
            )
            .task { appDelegate.bind(application) }
        } defaultValue: {
            WorkspaceID()
        }
        .commands {
            InkpodCommands(application: application)
        }

        WindowGroup("Batch", id: "inkpod.batch", for: BatchWindowID.self) { $batchID in
            if let batchID, let context = application.batchContext(for: batchID) {
                InkpodBatchWindowScene(
                    id: batchID,
                    context: context,
                    application: application
                )
            } else {
                ContentUnavailableView("Batch context unavailable", systemImage: "exclamationmark.triangle")
            }
        }

        Settings {
            InkpodSettingsView(application: application)
        }
    }
}

@MainActor
private final class InkpodAppDelegate: NSObject, NSApplicationDelegate {
    private weak var application: ApplicationCoordinator?
    private var terminationPending = false

    func bind(_ application: ApplicationCoordinator) {
        self.application = application
        application.startInputMonitoring()
    }

    func applicationShouldTerminateAfterLastWindowClosed(
        _ sender: NSApplication
    ) -> Bool {
        true
    }

    func applicationShouldTerminate(
        _ sender: NSApplication
    ) -> NSApplication.TerminateReply {
        guard !terminationPending, let application else {
            return terminationPending ? .terminateLater : .terminateNow
        }
        terminationPending = true
        Task { @MainActor in
            let shouldTerminate = await application.shutdown()
            terminationPending = false
            sender.reply(toApplicationShouldTerminate: shouldTerminate)
        }
        return .terminateLater
    }
}
