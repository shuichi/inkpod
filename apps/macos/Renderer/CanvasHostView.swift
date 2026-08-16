import AppKit
import ObjectiveC
import QuartzCore
import SwiftUI

@MainActor
final class WindowCloseGuard: NSObject, NSWindowDelegate {
    private weak var model: WorkspaceModel?
    // NSWindow does not retain its delegate. Keep the delegate installed by
    // SwiftUI alive while this forwarding guard owns the delegate slot.
    nonisolated(unsafe) private var previousDelegate: (any NSWindowDelegate)?
    private var resolvingDirtyState = false
    private var allowsNextClose = false

    init(model: WorkspaceModel, previousDelegate: (any NSWindowDelegate)?) {
        self.model = model
        self.previousDelegate = previousDelegate
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        if allowsNextClose {
            allowsNextClose = false
            return previousDelegate?.windowShouldClose?(sender) ?? true
        }
        guard model?.projection?.isDirty == true else {
            return previousDelegate?.windowShouldClose?(sender) ?? true
        }
        guard !resolvingDirtyState else { return false }
        resolvingDirtyState = true
        Task { [weak self, weak sender] in
            guard let self else { return }
            defer { self.resolvingDirtyState = false }
            guard let model = self.model,
                  await model.resolveDirtyBeforeClose(),
                  let sender
            else {
                return
            }
            self.allowsNextClose = true
            sender.performClose(nil)
        }
        return false
    }

    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || previousDelegate?.responds(to: selector) == true
    }

    override func forwardingTarget(for selector: Selector!) -> Any? {
        if previousDelegate?.responds(to: selector) == true {
            return previousDelegate
        }
        return super.forwardingTarget(for: selector)
    }

    func restore(on window: NSWindow) {
        let delegate = previousDelegate
        if window.delegate === self {
            window.delegate = delegate
        }
        previousDelegate = nil
    }

    static func retainForWindowLifetime(_ guard: WindowCloseGuard, on window: NSWindow) {
        let key = Unmanaged.passUnretained(WindowCloseGuard.self as AnyObject).toOpaque()
        objc_setAssociatedObject(window, key, `guard`, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
    }
}

@MainActor
final class CanvasHostView: NSView {
    private let model: WorkspaceModel
    private let viewID: WorkspaceViewID
    private var surface: CoreSurfaceTarget?
    private var trackingArea: NSTrackingArea?
    private var observerTokens: [NSObjectProtocol] = []
    private var strokeActive = false
    private var m8GestureActive = false
    private var panActive = false
    private var lastPanPoint: NSPoint?
    private var lastDrawableSize = CGSize.zero
    private weak var registeredWindow: NSWindow?
    private var closeGuard: WindowCloseGuard?
    private let floatingHandlesLayer = CAShapeLayer()
    private var floatingDrag: (start: NSPoint, draft: FloatingTransformDraft)?

    init(model: WorkspaceModel, viewID: WorkspaceViewID) {
        self.model = model
        self.viewID = viewID
        super.init(frame: .zero)
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
        floatingHandlesLayer.fillColor = NSColor.controlAccentColor.cgColor
        floatingHandlesLayer.strokeColor = NSColor.white.cgColor
        floatingHandlesLayer.lineWidth = 1.5
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Canvas")
        setAccessibilityIdentifier("inkpod.canvas")
        registerForDraggedTypes([.fileURL])
        updateAccessibilityProjection()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    override func makeBackingLayer() -> CALayer {
        CAMetalLayer()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let registeredWindow, registeredWindow !== window {
            closeGuard?.restore(on: registeredWindow)
            closeGuard = nil
            model.unregisterWindow(registeredWindow)
            self.registeredWindow = nil
        }
        removeWindowObservers()
        guard let window else {
            unregisterSurface()
            return
        }
        if registeredWindow !== window {
            model.registerWindow(window)
            registeredWindow = window
            let closeGuard = WindowCloseGuard(model: model, previousDelegate: window.delegate)
            self.closeGuard = closeGuard
            window.delegate = closeGuard
            WindowCloseGuard.retainForWindowLifetime(closeGuard, on: window)
        }
        observerTokens = [
            NotificationCenter.default.addObserver(
                forName: NSWindow.didChangeBackingPropertiesNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.backingOrDisplayChanged() }
            },
            NotificationCenter.default.addObserver(
                forName: NSWindow.didChangeOcclusionStateNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.publishVisibility() }
            },
            NotificationCenter.default.addObserver(
                forName: NSWindow.didBecomeKeyNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.publishVisibility() }
            },
            NotificationCenter.default.addObserver(
                forName: NSWindow.didMiniaturizeNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.publishVisibility() }
            },
            NotificationCenter.default.addObserver(
                forName: NSWindow.didDeminiaturizeNotification,
                object: window,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.publishVisibility() }
            },
        ]
        registerSurfaceIfNeeded()
        publishVisibility()
    }

    override func layout() {
        super.layout()
        updateFloatingHandles()
        guard surface != nil else {
            registerSurfaceIfNeeded()
            return
        }
        let size = drawableSize()
        guard valid(size), size != lastDrawableSize else { return }
        lastDrawableSize = size
        model.displayOrBackingChanged(size, viewID: viewID)
    }

    override func updateTrackingAreas() {
        if let trackingArea {
            removeTrackingArea(trackingArea)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.activeInKeyWindow, .mouseMoved, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        trackingArea = area
        super.updateTrackingAreas()
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        if let point = backingPoint(event),
           let draft = model.pendingFloatingTransform,
           model.floatingHandleDevicePoints(viewID: viewID).contains(where: {
               hypot($0.x - point.x, $0.y - point.y) <= 16
           })
        {
            floatingDrag = (point, draft)
            return
        }
        guard let sample = normalizedSample(event) else { return }
        if model.beginM8CanvasGesture(sample, viewID: viewID) {
            m8GestureActive = true
            return
        }
        strokeActive = true
        model.beginStroke(sample, viewID: viewID)
    }

    override func mouseMoved(with event: NSEvent) {
        guard let sample = normalizedSample(event) else { return }
        model.updateLocator(sample, viewID: viewID)
    }

    override func mouseDragged(with event: NSEvent) {
        if let drag = floatingDrag, let point = backingPoint(event), model.currentZoom > 0 {
            var replacement = drag.draft
            replacement.targetX += (point.x - drag.start.x) / model.currentZoom
            replacement.targetY += (point.y - drag.start.y) / model.currentZoom
            model.previewPendingPaste(replacement)
            return
        }
        guard let sample = normalizedSample(event) else { return }
        if m8GestureActive {
            model.appendM8CanvasGesture(sample, viewID: viewID)
            return
        }
        guard strokeActive else { return }
        model.appendStroke(sample, viewID: viewID)
    }

    override func mouseUp(with event: NSEvent) {
        if floatingDrag != nil {
            floatingDrag = nil
            return
        }
        if m8GestureActive {
            m8GestureActive = false
            model.endM8CanvasGesture(normalizedSample(event), viewID: viewID)
            return
        }
        guard strokeActive else { return }
        strokeActive = false
        let sample = normalizedSample(event)
        model.endStroke(finalSample: sample, viewID: viewID)
    }

    override func tabletPoint(with event: NSEvent) {
        guard strokeActive, let sample = normalizedSample(event) else {
            super.tabletPoint(with: event)
            return
        }
        model.appendStroke(sample, viewID: viewID)
    }

    override func otherMouseDown(with event: NSEvent) {
        guard event.buttonNumber == 2 else {
            super.otherMouseDown(with: event)
            return
        }
        window?.makeFirstResponder(self)
        panActive = true
        lastPanPoint = backingPoint(event)
    }

    override func otherMouseDragged(with event: NSEvent) {
        guard panActive,
              let previous = lastPanPoint,
              let current = backingPoint(event)
        else {
            return
        }
        lastPanPoint = current
        model.pan(
            deviceDX: current.x - previous.x,
            deviceDY: current.y - previous.y,
            viewID: viewID
        )
    }

    override func otherMouseUp(with event: NSEvent) {
        guard event.buttonNumber == 2 else {
            super.otherMouseUp(with: event)
            return
        }
        panActive = false
        lastPanPoint = nil
    }

    override func scrollWheel(with event: NSEvent) {
        let backingPixelsPerPoint = convertToBacking(NSSize(width: 1, height: 1))
        guard let delta = CanvasInputNormalizer.backingScrollDelta(
            pointDelta: CGVector(dx: event.scrollingDeltaX, dy: event.scrollingDeltaY),
            backingPixelsPerPoint: backingPixelsPerPoint
        )
        else {
            return
        }
        model.pan(deviceDX: delta.dx, deviceDY: delta.dy, viewID: viewID)
    }

    override func magnify(with event: NSEvent) {
        guard let anchor = backingPoint(event) else { return }
        let factor = max(0.1, 1 + event.magnification)
        model.zoom(factor: factor, deviceX: anchor.x, deviceY: anchor.y, viewID: viewID)
    }

    override func cancelOperation(_ sender: Any?) {
        if model.pendingFloatingTransform != nil {
            model.cancelPendingPaste()
        } else {
            cancelTransientInput()
        }
    }

    override func keyDown(with event: NSEvent) {
        guard model.pendingFloatingTransform != nil else {
            super.keyDown(with: event)
            return
        }
        let step = event.modifierFlags.contains(.shift) ? 10.0 : 1.0
        switch event.keyCode {
        case 123:
            model.nudgePendingPaste(documentDX: -step, documentDY: 0)
        case 124:
            model.nudgePendingPaste(documentDX: step, documentDY: 0)
        case 125:
            model.nudgePendingPaste(documentDX: 0, documentDY: step)
        case 126:
            model.nudgePendingPaste(documentDX: 0, documentDY: -step)
        case 36, 76:
            if let draft = model.pendingFloatingTransform {
                model.commitPendingTransform(draft)
            }
        case 53:
            model.cancelPendingPaste()
        default:
            super.keyDown(with: event)
        }
    }

    override func draggingEntered(_ sender: any NSDraggingInfo) -> NSDragOperation {
        draggedFileURL(from: sender).flatMap(FileTypeCatalog.classify) == nil ? [] : .copy
    }

    override func performDragOperation(_ sender: any NSDraggingInfo) -> Bool {
        guard let url = draggedFileURL(from: sender), FileTypeCatalog.classify(url) != nil else {
            return false
        }
        model.openDroppedURL(url)
        return true
    }

    override func resignFirstResponder() -> Bool {
        cancelTransientInput()
        return super.resignFirstResponder()
    }

    func dismantle() {
        cancelTransientInput()
        if let registeredWindow {
            closeGuard?.restore(on: registeredWindow)
            closeGuard = nil
            model.unregisterWindow(registeredWindow)
            self.registeredWindow = nil
        }
        unregisterSurface()
        removeWindowObservers()
    }

    func updateAccessibilityProjection() {
        guard let projection = model.projection else {
            setAccessibilityValue("starting")
            return
        }
        setAccessibilityValue(
            "documentRevision=\(projection.documentRevision);"
                + "viewRevision=\(projection.viewRevision);"
                + "presentedFrames=\(model.presentedFrameCount)"
        )
        setAccessibilityLabel(model.localizedText("canvas.accessibility.label"))
        updateFloatingHandles()
        updateFloatingAccessibilityActions()
    }

    private func updateFloatingHandles() {
        let devicePoints = model.floatingHandleDevicePoints(viewID: viewID)
        guard !devicePoints.isEmpty, let rootLayer = layer else {
            floatingHandlesLayer.removeFromSuperlayer()
            return
        }
        if floatingHandlesLayer.superlayer !== rootLayer {
            rootLayer.addSublayer(floatingHandlesLayer)
        }
        floatingHandlesLayer.frame = bounds
        let points = devicePoints.map { convertFromBacking(NSPoint(x: $0.x, y: $0.y)) }
        let path = CGMutablePath()
        if points.count == 5 {
            path.move(to: points[0])
            for index in [1, 4, 3] { path.addLine(to: points[index]) }
            path.closeSubpath()
        }
        for point in points {
            path.addEllipse(in: CGRect(x: point.x - 5, y: point.y - 5, width: 10, height: 10))
        }
        floatingHandlesLayer.path = path
    }

    private func updateFloatingAccessibilityActions() {
        guard model.pendingFloatingTransform != nil else {
            setAccessibilityCustomActions([])
            return
        }
        let actions: [(String, () -> Bool)] = [
            (model.localizedText("canvas.selection.move.left"), { [weak self] in
                self?.model.nudgePendingPaste(documentDX: -1, documentDY: 0)
                return self != nil
            }),
            (model.localizedText("canvas.selection.move.right"), { [weak self] in
                self?.model.nudgePendingPaste(documentDX: 1, documentDY: 0)
                return self != nil
            }),
            (model.localizedText("canvas.selection.move.up"), { [weak self] in
                self?.model.nudgePendingPaste(documentDX: 0, documentDY: -1)
                return self != nil
            }),
            (model.localizedText("canvas.selection.move.down"), { [weak self] in
                self?.model.nudgePendingPaste(documentDX: 0, documentDY: 1)
                return self != nil
            }),
            (model.localizedText("action.apply"), { [weak self] in
                guard let self, let draft = self.model.pendingFloatingTransform else { return false }
                self.model.commitPendingTransform(draft)
                return true
            }),
            (model.localizedText("action.cancel"), { [weak self] in
                guard let self else { return false }
                self.model.cancelPendingPaste()
                return true
            }),
        ]
        setAccessibilityCustomActions(actions.map {
            NSAccessibilityCustomAction(name: $0.0, handler: $0.1)
        })
    }

    private func registerSurfaceIfNeeded() {
        guard surface == nil,
              let metalLayer = layer as? CAMetalLayer
        else {
            return
        }
        let size = drawableSize()
        guard valid(size), let registered = model.registerCanvas(
            viewID: viewID,
            layer: metalLayer,
            drawableSize: size
        ) else {
            return
        }
        surface = registered
        lastDrawableSize = size
    }

    private func unregisterSurface() {
        guard let surface else { return }
        model.unregisterCanvas(surface)
        self.surface = nil
    }

    private func backingOrDisplayChanged() {
        let size = drawableSize()
        guard valid(size), surface != nil else { return }
        lastDrawableSize = size
        model.displayOrBackingChanged(size, viewID: viewID)
    }

    private func publishVisibility() {
        let visible = !isHidden && window?.isVisible == true
            && window?.isMiniaturized == false
            && window?.occlusionState.contains(.visible) == true
        model.setCanvasVisible(visible, viewID: viewID)
    }

    private func cancelTransientInput() {
        if strokeActive {
            strokeActive = false
            model.cancelStroke(viewID: viewID)
        }
        if m8GestureActive {
            m8GestureActive = false
            model.cancelM8CanvasGesture(viewID: viewID)
        }
        panActive = false
        lastPanPoint = nil
        floatingDrag = nil
    }

    private func normalizedSample(_ event: NSEvent) -> CorePointerSample? {
        guard let point = backingPoint(event) else { return nil }
        let size = drawableSize()
        let supportsTabletValues = event.type == .tabletPoint
            || event.subtype == .tabletPoint
        let pressure: Float? = switch event.type {
        case .leftMouseDown, .leftMouseDragged, .leftMouseUp, .tabletPoint, .pressure:
            event.pressure
        default:
            nil
        }
        let tilt = supportsTabletValues
            ? CanvasTilt(x: Float(event.tilt.x), y: Float(event.tilt.y))
            : nil
        return CanvasInputNormalizer.sample(
            deviceX: point.x,
            deviceY: point.y,
            drawableWidth: size.width,
            drawableHeight: size.height,
            pressure: pressure,
            tilt: tilt
        )
    }

    private func draggedFileURL(from sender: any NSDraggingInfo) -> URL? {
        let options: [NSPasteboard.ReadingOptionKey: Any] = [
            .urlReadingFileURLsOnly: true,
        ]
        return (sender.draggingPasteboard.readObjects(
            forClasses: [NSURL.self],
            options: options
        ) as? [URL])?.first
    }

    private func backingPoint(_ event: NSEvent) -> NSPoint? {
        guard event.window === window else { return nil }
        let rawPoint = convertToBacking(convert(event.locationInWindow, from: nil))
        return CanvasInputNormalizer.localDevicePoint(
            backingPoint: rawPoint,
            backingBounds: convertToBacking(bounds),
            isFlipped: isFlipped
        )
    }

    private func drawableSize() -> CGSize {
        convertToBacking(bounds).size
    }

    private func valid(_ size: CGSize) -> Bool {
        size.width.isFinite && size.height.isFinite && size.width > 0 && size.height > 0
    }

    private func removeWindowObservers() {
        for token in observerTokens {
            NotificationCenter.default.removeObserver(token)
        }
        observerTokens.removeAll(keepingCapacity: false)
    }
}

struct CanvasSurfaceView: NSViewRepresentable {
    @ObservedObject var model: WorkspaceModel
    let viewID: WorkspaceViewID

    init(model: WorkspaceModel, viewID: WorkspaceViewID? = nil) {
        self.model = model
        self.viewID = viewID ?? model.editorGraph?.activeView?.id ?? WorkspaceViewID(rawValue: 0)
    }

    func makeNSView(context: Context) -> CanvasHostView {
        CanvasHostView(model: model, viewID: viewID)
    }

    func updateNSView(_ nsView: CanvasHostView, context: Context) {
        nsView.updateAccessibilityProjection()
    }

    static func dismantleNSView(_ nsView: CanvasHostView, coordinator: ()) {
        nsView.dismantle()
    }
}
