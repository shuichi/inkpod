import Darwin
import CoreText
import Foundation
import InkpodCoreC
import Metal
import QuartzCore
import simd

struct MetalRendererMetrics: Equatable, Sendable {
    var executionThreadID: UInt64 = 0
    var surfaceCount = 0
    var presentedFrameCount: UInt64 = 0
    var drawableUnavailableCount: UInt64 = 0
    var uploadedTileCount: UInt64 = 0
    var reusedTileCount: UInt64 = 0
    var rejectedSnapshotCount: UInt64 = 0
    var replacedSnapshotCount: UInt64 = 0
    var hiddenDrawCount: UInt64 = 0
    var deviceRebuildCount: UInt64 = 0
    var memoryPressurePurgeCount: UInt64 = 0
}

private final class MetalRendererMetricsStore: @unchecked Sendable {
    private let lock = NSLock()
    private var value = MetalRendererMetrics()

    func publishRenderer(_ renderer: MetalRendererMetrics) {
        lock.withLock {
            value.executionThreadID = renderer.executionThreadID
            value.surfaceCount = renderer.surfaceCount
            value.presentedFrameCount = renderer.presentedFrameCount
            value.drawableUnavailableCount = renderer.drawableUnavailableCount
            value.uploadedTileCount = renderer.uploadedTileCount
            value.reusedTileCount = renderer.reusedTileCount
            value.hiddenDrawCount = renderer.hiddenDrawCount
            value.deviceRebuildCount = renderer.deviceRebuildCount
            value.memoryPressurePurgeCount = renderer.memoryPressurePurgeCount
        }
    }

    func recordRejectedSnapshot() {
        lock.withLock { value.rejectedSnapshotCount += 1 }
    }

    func recordReplacedSnapshot() {
        lock.withLock { value.replacedSnapshotCount += 1 }
    }

    func snapshot() -> MetalRendererMetrics {
        lock.withLock { value }
    }
}

private final class MetalLayerReference: @unchecked Sendable {
    let layer: CAMetalLayer

    init(_ layer: CAMetalLayer) {
        self.layer = layer
    }
}

private final class RendererCommandCompletion: @unchecked Sendable {
    private let condition = NSCondition()
    private var value: Bool?

    func complete(_ value: Bool) {
        condition.lock()
        precondition(self.value == nil)
        self.value = value
        condition.broadcast()
        condition.unlock()
    }

    func wait(timeout: TimeInterval) -> Bool {
        let deadline = Date(timeIntervalSinceNow: timeout)
        condition.lock()
        while value == nil, condition.wait(until: deadline) {}
        let result = value ?? false
        condition.unlock()
        return result
    }
}

private enum RendererCommand: @unchecked Sendable {
    case register(
        route: CoreSnapshotRoute,
        layer: MetalLayerReference,
        drawableSize: CGSize,
        completion: RendererCommandCompletion
    )
    case resize(CoreSurfaceTarget, CGSize)
    case visibility(CoreSurfaceTarget, Bool)
    case rebuildDevice(CoreSurfaceTarget?)
    case memoryPressure
    case unregister(CoreSurfaceTarget, RendererCommandCompletion)
    case barrier(RendererCommandCompletion)
    case shutdown(RendererCommandCompletion)
}

private struct RendererCommandRing {
    private var storage: [RendererCommand?]
    private var head = 0
    private var tail = 0
    private(set) var count = 0

    init(capacity: Int) {
        storage = Array(repeating: nil, count: capacity)
    }

    mutating func append(_ command: RendererCommand) -> Bool {
        guard count < storage.count else { return false }
        storage[tail] = command
        tail = (tail + 1) % storage.count
        count += 1
        return true
    }

    mutating func popFirst() -> RendererCommand? {
        guard count > 0 else { return nil }
        let command = storage[head]
        storage[head] = nil
        head = (head + 1) % storage.count
        count -= 1
        return command
    }
}

private enum RendererWork {
    case command(RendererCommand)
    case render
}

private final class RendererMailbox: @unchecked Sendable {
    private let condition = NSCondition()
    private var commands = RendererCommandRing(capacity: 128)
    private var renderRequested = false
    private var stopped = false

    func enqueue(_ command: RendererCommand) -> Bool {
        condition.lock()
        guard !stopped, commands.append(command) else {
            condition.unlock()
            return false
        }
        condition.signal()
        condition.unlock()
        return true
    }

    func requestRender() {
        condition.lock()
        if !stopped {
            renderRequested = true
            condition.signal()
        }
        condition.unlock()
    }

    func take() -> RendererWork? {
        condition.lock()
        defer { condition.unlock() }
        while true {
            if let command = commands.popFirst() {
                return .command(command)
            }
            if renderRequested {
                renderRequested = false
                return .render
            }
            if stopped {
                return nil
            }
            condition.wait()
        }
    }

    func stop() {
        condition.lock()
        stopped = true
        condition.broadcast()
        condition.unlock()
    }
}

final class MetalRendererHost: @unchecked Sendable {
    private let ownershipQueue = SnapshotOwnershipQueue(capacity: 64)
    private let mailbox = RendererMailbox()
    private let metricsStore = MetalRendererMetricsStore()
    private let finish = RendererCommandCompletion()
    private let thread: Thread

    init() {
        let ownershipQueue = self.ownershipQueue
        let mailbox = self.mailbox
        let metricsStore = self.metricsStore
        let finish = self.finish
        let shaderSource = Self.loadShaderSource()
        thread = Thread {
            let loop = MetalRendererLoop(
                ownershipQueue: ownershipQueue,
                mailbox: mailbox,
                metricsStore: metricsStore,
                shaderSource: shaderSource
            )
            loop.run()
            finish.complete(true)
        }
        thread.name = "inkpod.macos.metal-renderer"
        thread.qualityOfService = .userInteractive
        thread.start()
    }

    private static func loadShaderSource() -> String? {
        let bundles = [Bundle.main] + Bundle.allBundles
        for bundle in bundles {
            guard let url = bundle.url(
                forResource: "CanvasShaders",
                withExtension: "metal"
            ) else {
                continue
            }
            if let source = try? String(contentsOf: url, encoding: .utf8) {
                return source
            }
        }
        return nil
    }

    func registerSurface(
        route: CoreSnapshotRoute,
        layer: CAMetalLayer,
        drawableSize: CGSize
    ) -> Bool {
        let completion = RendererCommandCompletion()
        guard mailbox.enqueue(
            .register(
                route: route,
                layer: MetalLayerReference(layer),
                drawableSize: drawableSize,
                completion: completion
            )
        ) else {
            return false
        }
        return completion.wait(timeout: 5)
    }

    func resizeSurface(_ surface: CoreSurfaceTarget, drawableSize: CGSize) {
        _ = mailbox.enqueue(.resize(surface, drawableSize))
    }

    func setSurfaceVisible(_ surface: CoreSurfaceTarget, visible: Bool) {
        _ = mailbox.enqueue(.visibility(surface, visible))
    }

    @discardableResult
    func submit(_ envelope: CoreSnapshotEnvelope) -> SnapshotSubmissionResult {
        let result = ownershipQueue.submit(envelope)
        switch result {
        case .accepted:
            mailbox.requestRender()
        case .replacedPending:
            metricsStore.recordReplacedSnapshot()
            mailbox.requestRender()
        case .rejectedStaleRoute, .rejectedHidden, .rejectedCapacity, .rejectedStopped:
            metricsStore.recordRejectedSnapshot()
        }
        return result
    }

    func handleDisplayOrDeviceChange(_ surface: CoreSurfaceTarget? = nil) {
        _ = mailbox.enqueue(.rebuildDevice(surface))
    }

    func handleMemoryPressure() {
        _ = mailbox.enqueue(.memoryPressure)
    }

    func unregisterSurface(_ surface: CoreSurfaceTarget) -> Bool {
        let completion = RendererCommandCompletion()
        guard mailbox.enqueue(.unregister(surface, completion)) else {
            return false
        }
        return completion.wait(timeout: 5)
    }

    func waitUntilIdle(timeout: TimeInterval) -> Bool {
        let completion = RendererCommandCompletion()
        guard mailbox.enqueue(.barrier(completion)) else { return false }
        return completion.wait(timeout: timeout)
    }

    func metrics() -> MetalRendererMetrics {
        metricsStore.snapshot()
    }

    func shutdown() -> Bool {
        let completion = RendererCommandCompletion()
        guard mailbox.enqueue(.shutdown(completion)) else {
            return finish.wait(timeout: 5)
        }
        guard completion.wait(timeout: 5) else { return false }
        return finish.wait(timeout: 5)
    }
}

private struct MetalSurfaceState {
    var route: CoreSnapshotRoute
    let layer: CAMetalLayer
    var drawableSize: CGSize
    var visible = true
}

private struct MetalTileCacheKey: Hashable {
    let session: CoreSessionTarget
    let tileID: UInt64
    let revision: UInt64
}

private struct MetalVertex {
    let position: SIMD2<Float>
    let textureCoordinate: SIMD2<Float>
}

private final class MetalRendererLoop {
    private let ownershipQueue: SnapshotOwnershipQueue
    private let mailbox: RendererMailbox
    private let metricsStore: MetalRendererMetricsStore
    private let shaderSource: String?
    private let device: any MTLDevice
    private let commandQueue: any MTLCommandQueue
    private var pipeline: (any MTLRenderPipelineState)?
    private var opacityPipeline: (any MTLRenderPipelineState)?
    private var solidPipeline: (any MTLRenderPipelineState)?
    private var lutPipeline: (any MTLRenderPipelineState)?
    private var stencilPipeline: (any MTLRenderPipelineState)?
    private var stencilFillPipeline: (any MTLRenderPipelineState)?
    private var stencilInvertState: (any MTLDepthStencilState)?
    private var stencilFillState: (any MTLDepthStencilState)?
    private var surfaces: [CoreSurfaceID: MetalSurfaceState] = [:]
    private var tileCache: [MetalTileCacheKey: any MTLTexture] = [:]
    private var metrics = MetalRendererMetrics()

    init(
        ownershipQueue: SnapshotOwnershipQueue,
        mailbox: RendererMailbox,
        metricsStore: MetalRendererMetricsStore,
        shaderSource: String?
    ) {
        guard let device = MTLCreateSystemDefaultDevice(),
              let commandQueue = device.makeCommandQueue()
        else {
            preconditionFailure("Metal is required by the macOS product target")
        }
        self.device = device
        self.commandQueue = commandQueue
        self.ownershipQueue = ownershipQueue
        self.mailbox = mailbox
        self.metricsStore = metricsStore
        self.shaderSource = shaderSource
        metrics.executionThreadID = UInt64(pthread_mach_thread_np(pthread_self()))
        metricsStore.publishRenderer(metrics)
    }

    func run() {
        while let work = mailbox.take() {
            switch work {
            case let .command(command):
                if execute(command) {
                    return
                }
            case .render:
                renderPendingSnapshots()
            }
            metrics.surfaceCount = surfaces.count
            metricsStore.publishRenderer(metrics)
        }
    }

    private func execute(_ command: RendererCommand) -> Bool {
        switch command {
        case let .register(route, layerReference, drawableSize, completion):
            guard validDrawableSize(drawableSize),
                  surfaces[route.surface.id] == nil,
                  ownershipQueue.registerSurface(route.surface, binding: route)
            else {
                completion.complete(false)
                return false
            }
            let layer = layerReference.layer
            layer.device = device
            layer.pixelFormat = .bgra8Unorm_srgb
            layer.framebufferOnly = true
            layer.maximumDrawableCount = 3
            layer.allowsNextDrawableTimeout = true
            layer.displaySyncEnabled = true
            layer.drawableSize = drawableSize
            surfaces[route.surface.id] = MetalSurfaceState(
                route: route,
                layer: layer,
                drawableSize: drawableSize
            )
            completion.complete(true)
        case let .resize(surface, drawableSize):
            guard validDrawableSize(drawableSize),
                  var state = surfaces[surface.id],
                  state.route.surface == surface
            else {
                return false
            }
            state.drawableSize = drawableSize
            state.layer.drawableSize = drawableSize
            surfaces[surface.id] = state
            renderRetained(surface)
        case let .visibility(surface, visible):
            guard var state = surfaces[surface.id], state.route.surface == surface else {
                return false
            }
            state.visible = visible
            surfaces[surface.id] = state
            ownershipQueue.setSurfaceVisible(surface, visible: visible)
            if visible {
                renderRetained(surface)
            }
        case let .rebuildDevice(surface):
            tileCache.removeAll(keepingCapacity: false)
            pipeline = nil
            opacityPipeline = nil
            solidPipeline = nil
            lutPipeline = nil
            stencilPipeline = nil
            stencilFillPipeline = nil
            stencilInvertState = nil
            stencilFillState = nil
            metrics.deviceRebuildCount += 1
            let targets = surface.map { [$0] }
                ?? surfaces.values.map(\.route.surface)
            for target in targets {
                guard let state = surfaces[target.id], state.route.surface == target else {
                    continue
                }
                state.layer.device = device
                renderRetained(target)
            }
        case .memoryPressure:
            tileCache.removeAll(keepingCapacity: false)
            metrics.memoryPressurePurgeCount += 1
        case let .unregister(surface, completion):
            guard let state = surfaces[surface.id], state.route.surface == surface else {
                completion.complete(false)
                return false
            }
            ownershipQueue.closeSurface(surface)
            surfaces.removeValue(forKey: surface.id)
            tileCache = tileCache.filter { $0.key.session != state.route.session }
            completion.complete(true)
        case let .barrier(completion):
            renderPendingSnapshots()
            completion.complete(true)
        case let .shutdown(completion):
            ownershipQueue.shutdown()
            surfaces.removeAll(keepingCapacity: false)
            tileCache.removeAll(keepingCapacity: false)
            pipeline = nil
            opacityPipeline = nil
            solidPipeline = nil
            lutPipeline = nil
            stencilPipeline = nil
            stencilFillPipeline = nil
            stencilInvertState = nil
            stencilFillState = nil
            completion.complete(true)
            mailbox.stop()
            return true
        }
        return false
    }

    private func renderPendingSnapshots() {
        while let envelope = ownershipQueue.takeNext() {
            guard let surface = surfaces[envelope.route.surface.id],
                  surface.route == envelope.route,
                  surface.visible
            else {
                envelope.owner.release()
                metricsStore.recordRejectedSnapshot()
                continue
            }
            _ = render(envelope, on: surface)
            _ = ownershipQueue.retainRendered(envelope)
        }
    }

    private func renderRetained(_ surface: CoreSurfaceTarget) {
        guard let state = surfaces[surface.id],
              state.route.surface == surface,
              state.visible,
              let envelope = ownershipQueue.retainedSnapshot(for: surface)
        else {
            return
        }
        _ = render(envelope, on: state)
    }

    @discardableResult
    private func render(
        _ envelope: CoreSnapshotEnvelope,
        on surface: MetalSurfaceState
    ) -> Bool {
        guard surface.visible else {
            metrics.hiddenDrawCount += 1
            return false
        }
        do {
            return try envelope.owner.withBorrowedRenderView { snapshot in
                guard let drawable = surface.layer.nextDrawable() else {
                    metrics.drawableUnavailableCount += 1
                    return false
                }
                guard let pipeline = try ensurePipeline(),
                      let opacityPipeline = try ensureOpacityPipeline(),
                      let solidPipeline = try ensureSolidPipeline(),
                      let lutPipeline = try ensureLUTPipeline(),
                      let stencilPipeline = try ensureStencilPipeline(),
                      let stencilFillPipeline = try ensureStencilFillPipeline(),
                      let stencilInvertState = ensureStencilInvertState(),
                      let stencilFillState = ensureStencilFillState(),
                      let commandBuffer = commandQueue.makeCommandBuffer(),
                      let primary = makeCanvasTexture(surface.drawableSize),
                      let secondary = makeCanvasTexture(surface.drawableSize),
                      let layerTexture = makeCanvasTexture(surface.drawableSize),
                      let stencilTexture = makeStencilTexture(surface.drawableSize)
                else {
                    return false
                }
                let whiteBase = snapshot.featureFlags
                    & inkpod_bridge_snapshot_solid_white_base() != 0
                let clearColor = whiteBase
                    ? MTLClearColor(red: 1, green: 1, blue: 1, alpha: 1)
                    : MTLClearColor(red: 0.18, green: 0.18, blue: 0.18, alpha: 1)
                var viewport = SIMD2<Float>(
                    Float(surface.drawableSize.width),
                    Float(surface.drawableSize.height)
                )
                let sampler = makeSampler()
                guard let initial = makeEncoder(
                    commandBuffer: commandBuffer,
                    texture: primary,
                    loadAction: .clear,
                    clearColor: clearColor
                ) else { return false }
                initial.endEncoding()

                var source = primary
                var destination = secondary
                var activeLayerOpacity: UInt32?
                let plan = snapshot.renderPasses.isEmpty
                    ? [CoreSnapshotRenderPassValue(
                        kind: 2,
                        layerID: 0,
                        planeID: 0,
                        opacityMilli: 1_000,
                        firstItem: 0,
                        itemCount: snapshot.tiles.count
                    )]
                    : snapshot.renderPasses
                for renderPass in plan {
                    switch renderPass.kind {
                    case 1:
                        guard activeLayerOpacity == nil,
                              let encoder = makeEncoder(
                                  commandBuffer: commandBuffer,
                                  texture: layerTexture,
                                  loadAction: .clear,
                                  clearColor: MTLClearColor(red: 0, green: 0, blue: 0, alpha: 0)
                              )
                        else { return false }
                        encoder.endEncoding()
                        activeLayerOpacity = renderPass.opacityMilli
                    case 2:
                        guard let encoder = makeEncoder(
                            commandBuffer: commandBuffer,
                            texture: activeLayerOpacity == nil ? source : layerTexture,
                            loadAction: .load,
                            clearColor: clearColor,
                            stencilTexture: stencilTexture
                        ) else { return false }
                        configure(
                            encoder,
                            pipeline: opacityPipeline,
                            viewport: &viewport,
                            sampler: sampler
                        )
                        var opacity = Float(renderPass.opacityMilli) / 1_000
                        encoder.setFragmentBytes(
                            &opacity,
                            length: MemoryLayout<Float>.stride,
                            index: 0
                        )
                        for tile in boundedSlice(
                            snapshot.tiles,
                            start: renderPass.firstItem,
                            count: renderPass.itemCount
                        ) {
                            guard let texture = texture(
                                for: tile,
                                session: envelope.route.session
                            ) else { continue }
                            var vertices = vertices(for: tile, transform: snapshot.transform)
                            encodeTexturedQuad(encoder, vertices: &vertices, texture: texture)
                        }
                        encoder.endEncoding()
                    case 3:
                        guard let encoder = makeEncoder(
                            commandBuffer: commandBuffer,
                            texture: activeLayerOpacity == nil ? source : layerTexture,
                            loadAction: .load,
                            clearColor: clearColor
                        ) else { return false }
                        configure(
                            encoder,
                            pipeline: solidPipeline,
                            viewport: &viewport,
                            sampler: nil
                        )
                        encodeVectorFills(
                            boundedSlice(
                                snapshot.vectorFills,
                                start: renderPass.firstItem,
                                count: renderPass.itemCount
                            ),
                            allSegments: snapshot.vectorSegments,
                            opacityMilli: renderPass.opacityMilli,
                            transform: snapshot.transform,
                            encoder: encoder,
                            stencilPipeline: stencilPipeline,
                            stencilFillPipeline: stencilFillPipeline,
                            stencilInvertState: stencilInvertState,
                            stencilFillState: stencilFillState,
                            viewport: &viewport
                        )
                        encoder.endEncoding()
                    case 4:
                        guard let encoder = makeEncoder(
                            commandBuffer: commandBuffer,
                            texture: activeLayerOpacity == nil ? source : layerTexture,
                            loadAction: .load,
                            clearColor: clearColor
                        ) else { return false }
                        configure(
                            encoder,
                            pipeline: solidPipeline,
                            viewport: &viewport,
                            sampler: nil
                        )
                        encodeVectorSegments(
                            boundedSlice(
                                snapshot.vectorSegments,
                                start: renderPass.firstItem,
                                count: renderPass.itemCount
                            ),
                            opacityMilli: renderPass.opacityMilli,
                            transform: snapshot.transform,
                            encoder: encoder
                        )
                        encoder.endEncoding()
                    case 5:
                        guard activeLayerOpacity == nil,
                              let lut = boundedSlice(
                            snapshot.adjustmentLUTs,
                            start: renderPass.firstItem,
                            count: max(renderPass.itemCount, 1)
                        ).first,
                        lut.count == 768,
                        let encoder = makeEncoder(
                            commandBuffer: commandBuffer,
                            texture: destination,
                            loadAction: .dontCare,
                            clearColor: clearColor
                        ) else { return false }
                        configure(
                            encoder,
                            pipeline: lutPipeline,
                            viewport: &viewport,
                            sampler: sampler
                        )
                        var vertices = fullScreenVertices(surface.drawableSize)
                        encoder.setVertexBytes(
                            &vertices,
                            length: MemoryLayout<MetalVertex>.stride * vertices.count,
                            index: 0
                        )
                        encoder.setFragmentTexture(source, index: 0)
                        lut.withUnsafeBytes { bytes in
                            encoder.setFragmentBytes(
                                bytes.baseAddress!,
                                length: bytes.count,
                                index: 0
                            )
                        }
                        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 6)
                        encoder.endEncoding()
                        swap(&source, &destination)
                    case 6:
                        guard let encoder = makeEncoder(
                            commandBuffer: commandBuffer,
                            texture: activeLayerOpacity == nil ? source : layerTexture,
                            loadAction: .load,
                            clearColor: clearColor
                        ) else { return false }
                        configure(
                            encoder,
                            pipeline: opacityPipeline,
                            viewport: &viewport,
                            sampler: sampler
                        )
                        encodeAnnotations(
                            boundedSlice(
                                snapshot.annotations,
                                start: renderPass.firstItem,
                                count: renderPass.itemCount
                            ),
                            opacityMilli: renderPass.opacityMilli,
                            transform: snapshot.transform,
                            encoder: encoder,
                            solidPipeline: solidPipeline,
                            opacityPipeline: opacityPipeline,
                            viewport: &viewport,
                            sampler: sampler
                        )
                        encoder.endEncoding()
                    case 7:
                        guard let layerOpacity = activeLayerOpacity,
                              let encoder = makeEncoder(
                                  commandBuffer: commandBuffer,
                                  texture: source,
                                  loadAction: .load,
                                  clearColor: clearColor
                              )
                        else { return false }
                        configure(
                            encoder,
                            pipeline: opacityPipeline,
                            viewport: &viewport,
                            sampler: sampler
                        )
                        var opacity = Float(layerOpacity) / 1_000
                        encoder.setFragmentBytes(
                            &opacity,
                            length: MemoryLayout<Float>.stride,
                            index: 0
                        )
                        var vertices = fullScreenVertices(surface.drawableSize)
                        encodeTexturedQuad(encoder, vertices: &vertices, texture: layerTexture)
                        encoder.endEncoding()
                        activeLayerOpacity = nil
                    default:
                        return false
                    }
                }

                guard activeLayerOpacity == nil else { return false }

                guard let overlay = makeEncoder(
                    commandBuffer: commandBuffer,
                    texture: source,
                    loadAction: .load,
                    clearColor: clearColor
                ) else { return false }
                configure(
                    overlay,
                    pipeline: solidPipeline,
                    viewport: &viewport,
                    sampler: nil
                )
                encodeGuides(snapshot, transform: snapshot.transform, encoder: overlay)
                overlay.endEncoding()

                guard let final = makeEncoder(
                    commandBuffer: commandBuffer,
                    texture: drawable.texture,
                    loadAction: .dontCare,
                    clearColor: clearColor
                ) else { return false }
                configure(final, pipeline: pipeline, viewport: &viewport, sampler: sampler)
                var finalVertices = fullScreenVertices(surface.drawableSize)
                encodeTexturedQuad(final, vertices: &finalVertices, texture: source)
                final.endEncoding()
                commandBuffer.present(drawable)
                commandBuffer.commit()
                metrics.presentedFrameCount += 1
                return true
            }
        } catch {
            metricsStore.recordRejectedSnapshot()
            return false
        }
    }

    private func ensurePipeline() throws -> (any MTLRenderPipelineState)? {
        if let pipeline {
            return pipeline
        }
        guard let shaderSource else {
            return nil
        }
        let library = try device.makeLibrary(source: shaderSource, options: nil)
        guard let vertex = library.makeFunction(name: "inkpodCanvasVertex"),
              let fragment = library.makeFunction(name: "inkpodCanvasFragment")
        else {
            return nil
        }
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertex
        descriptor.fragmentFunction = fragment
        descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
        descriptor.colorAttachments[0].isBlendingEnabled = true
        descriptor.colorAttachments[0].sourceRGBBlendFactor = .one
        descriptor.colorAttachments[0].destinationRGBBlendFactor = .oneMinusSourceAlpha
        descriptor.colorAttachments[0].sourceAlphaBlendFactor = .one
        descriptor.colorAttachments[0].destinationAlphaBlendFactor = .oneMinusSourceAlpha
        let created = try device.makeRenderPipelineState(descriptor: descriptor)
        pipeline = created
        return created
    }

    private func ensureOpacityPipeline() throws -> (any MTLRenderPipelineState)? {
        if let opacityPipeline { return opacityPipeline }
        let created = try makePipeline(fragment: "inkpodCanvasOpacityFragment")
        opacityPipeline = created
        return created
    }

    private func ensureSolidPipeline() throws -> (any MTLRenderPipelineState)? {
        if let solidPipeline { return solidPipeline }
        let created = try makePipeline(fragment: "inkpodCanvasSolidFragment")
        solidPipeline = created
        return created
    }

    private func ensureLUTPipeline() throws -> (any MTLRenderPipelineState)? {
        if let lutPipeline { return lutPipeline }
        let created = try makePipeline(fragment: "inkpodCanvasLUTFragment", blending: false)
        lutPipeline = created
        return created
    }

    private func ensureStencilPipeline() throws -> (any MTLRenderPipelineState)? {
        if let stencilPipeline { return stencilPipeline }
        guard let shaderSource else { return nil }
        let library = try device.makeLibrary(source: shaderSource, options: nil)
        guard let vertex = library.makeFunction(name: "inkpodCanvasVertex"),
              let fragment = library.makeFunction(name: "inkpodCanvasSolidFragment")
        else { return nil }
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertex
        descriptor.fragmentFunction = fragment
        descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
        descriptor.colorAttachments[0].writeMask = []
        descriptor.stencilAttachmentPixelFormat = .stencil8
        let created = try device.makeRenderPipelineState(descriptor: descriptor)
        stencilPipeline = created
        return created
    }

    private func ensureStencilFillPipeline() throws -> (any MTLRenderPipelineState)? {
        if let stencilFillPipeline { return stencilFillPipeline }
        guard let shaderSource else { return nil }
        let library = try device.makeLibrary(source: shaderSource, options: nil)
        guard let vertex = library.makeFunction(name: "inkpodCanvasVertex"),
              let fragment = library.makeFunction(name: "inkpodCanvasSolidFragment")
        else { return nil }
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertex
        descriptor.fragmentFunction = fragment
        descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
        descriptor.colorAttachments[0].isBlendingEnabled = true
        descriptor.colorAttachments[0].sourceRGBBlendFactor = .one
        descriptor.colorAttachments[0].destinationRGBBlendFactor = .oneMinusSourceAlpha
        descriptor.colorAttachments[0].sourceAlphaBlendFactor = .one
        descriptor.colorAttachments[0].destinationAlphaBlendFactor = .oneMinusSourceAlpha
        descriptor.stencilAttachmentPixelFormat = .stencil8
        let created = try device.makeRenderPipelineState(descriptor: descriptor)
        stencilFillPipeline = created
        return created
    }

    private func ensureStencilInvertState() -> (any MTLDepthStencilState)? {
        if let stencilInvertState { return stencilInvertState }
        let stencil = MTLStencilDescriptor()
        stencil.stencilCompareFunction = .always
        stencil.stencilFailureOperation = .keep
        stencil.depthFailureOperation = .keep
        stencil.depthStencilPassOperation = .invert
        stencil.readMask = 0xff
        stencil.writeMask = 0xff
        let descriptor = MTLDepthStencilDescriptor()
        descriptor.frontFaceStencil = stencil
        descriptor.backFaceStencil = stencil
        let created = device.makeDepthStencilState(descriptor: descriptor)
        stencilInvertState = created
        return created
    }

    private func ensureStencilFillState() -> (any MTLDepthStencilState)? {
        if let stencilFillState { return stencilFillState }
        let stencil = MTLStencilDescriptor()
        stencil.stencilCompareFunction = .notEqual
        stencil.stencilFailureOperation = .keep
        stencil.depthFailureOperation = .keep
        stencil.depthStencilPassOperation = .keep
        stencil.readMask = 0xff
        stencil.writeMask = 0
        let descriptor = MTLDepthStencilDescriptor()
        descriptor.frontFaceStencil = stencil
        descriptor.backFaceStencil = stencil
        let created = device.makeDepthStencilState(descriptor: descriptor)
        stencilFillState = created
        return created
    }

    private func makePipeline(
        fragment fragmentName: String,
        blending: Bool = true
    ) throws -> (any MTLRenderPipelineState)? {
        guard let shaderSource else { return nil }
        let library = try device.makeLibrary(source: shaderSource, options: nil)
        guard let vertex = library.makeFunction(name: "inkpodCanvasVertex"),
              let fragment = library.makeFunction(name: fragmentName)
        else { return nil }
        let descriptor = MTLRenderPipelineDescriptor()
        descriptor.vertexFunction = vertex
        descriptor.fragmentFunction = fragment
        descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm_srgb
        descriptor.colorAttachments[0].isBlendingEnabled = blending
        descriptor.colorAttachments[0].sourceRGBBlendFactor = .one
        descriptor.colorAttachments[0].destinationRGBBlendFactor = .oneMinusSourceAlpha
        descriptor.colorAttachments[0].sourceAlphaBlendFactor = .one
        descriptor.colorAttachments[0].destinationAlphaBlendFactor = .oneMinusSourceAlpha
        return try device.makeRenderPipelineState(descriptor: descriptor)
    }

    private func makeSampler() -> (any MTLSamplerState)? {
        let descriptor = MTLSamplerDescriptor()
        descriptor.minFilter = .nearest
        descriptor.magFilter = .nearest
        descriptor.sAddressMode = .clampToEdge
        descriptor.tAddressMode = .clampToEdge
        return device.makeSamplerState(descriptor: descriptor)
    }

    private func texture(
        for tile: BorrowedCoreSnapshotTile,
        session: CoreSessionTarget
    ) -> (any MTLTexture)? {
        let key = MetalTileCacheKey(
            session: session,
            tileID: tile.tileID,
            revision: tile.tileRevision
        )
        if let cached = tileCache[key] {
            metrics.reusedTileCount += 1
            return cached
        }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm_srgb,
            width: Int(tile.width),
            height: Int(tile.height),
            mipmapped: false
        )
        descriptor.storageMode = .managed
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor),
              let pixels = tile.pixels.baseAddress
        else {
            return nil
        }
        texture.replace(
            region: MTLRegionMake2D(0, 0, Int(tile.width), Int(tile.height)),
            mipmapLevel: 0,
            withBytes: pixels,
            bytesPerRow: Int(tile.strideBytes)
        )
        tileCache[key] = texture
        metrics.uploadedTileCount += 1
        return texture
    }

    private func vertices(
        for tile: BorrowedCoreSnapshotTile,
        transform: CoreSnapshotTransform
    ) -> [MetalVertex] {
        let p0 = devicePoint(
            SIMD2(Float(tile.originX), Float(tile.originY)),
            transform: transform
        )
        let p1 = devicePoint(
            SIMD2(
                Float(tile.originX + Int32(tile.width)),
                Float(tile.originY + Int32(tile.height))
            ),
            transform: transform
        )
        let x0 = min(p0.x, p1.x)
        let y0 = min(p0.y, p1.y)
        let x1 = max(p0.x, p1.x)
        let y1 = max(p0.y, p1.y)
        let topLeft = MetalVertex(position: .init(x0, y0), textureCoordinate: .init(0, 0))
        let topRight = MetalVertex(position: .init(x1, y0), textureCoordinate: .init(1, 0))
        let bottomLeft = MetalVertex(position: .init(x0, y1), textureCoordinate: .init(0, 1))
        let bottomRight = MetalVertex(position: .init(x1, y1), textureCoordinate: .init(1, 1))
        return [topLeft, bottomLeft, topRight, topRight, bottomLeft, bottomRight]
    }

    private func makeCanvasTexture(_ size: CGSize) -> (any MTLTexture)? {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm_srgb,
            width: Int(size.width),
            height: Int(size.height),
            mipmapped: false
        )
        descriptor.storageMode = .private
        descriptor.usage = [.renderTarget, .shaderRead]
        return device.makeTexture(descriptor: descriptor)
    }

    private func makeStencilTexture(_ size: CGSize) -> (any MTLTexture)? {
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .stencil8,
            width: Int(size.width),
            height: Int(size.height),
            mipmapped: false
        )
        descriptor.storageMode = .private
        descriptor.usage = .renderTarget
        return device.makeTexture(descriptor: descriptor)
    }

    private func makeEncoder(
        commandBuffer: any MTLCommandBuffer,
        texture: any MTLTexture,
        loadAction: MTLLoadAction,
        clearColor: MTLClearColor,
        stencilTexture: (any MTLTexture)? = nil
    ) -> (any MTLRenderCommandEncoder)? {
        let descriptor = MTLRenderPassDescriptor()
        descriptor.colorAttachments[0].texture = texture
        descriptor.colorAttachments[0].loadAction = loadAction
        descriptor.colorAttachments[0].storeAction = .store
        descriptor.colorAttachments[0].clearColor = clearColor
        if let stencilTexture {
            descriptor.stencilAttachment.texture = stencilTexture
            descriptor.stencilAttachment.loadAction = .clear
            descriptor.stencilAttachment.storeAction = .dontCare
            descriptor.stencilAttachment.clearStencil = 0
        }
        return commandBuffer.makeRenderCommandEncoder(descriptor: descriptor)
    }

    private func configure(
        _ encoder: any MTLRenderCommandEncoder,
        pipeline: any MTLRenderPipelineState,
        viewport: inout SIMD2<Float>,
        sampler: (any MTLSamplerState)?
    ) {
        encoder.setRenderPipelineState(pipeline)
        encoder.setVertexBytes(
            &viewport,
            length: MemoryLayout<SIMD2<Float>>.stride,
            index: 1
        )
        if let sampler { encoder.setFragmentSamplerState(sampler, index: 0) }
    }

    private func encodeTexturedQuad(
        _ encoder: any MTLRenderCommandEncoder,
        vertices: inout [MetalVertex],
        texture: any MTLTexture
    ) {
        encoder.setVertexBytes(
            &vertices,
            length: MemoryLayout<MetalVertex>.stride * vertices.count,
            index: 0
        )
        encoder.setFragmentTexture(texture, index: 0)
        encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 6)
    }

    private func fullScreenVertices(_ size: CGSize) -> [MetalVertex] {
        let x = Float(size.width)
        let y = Float(size.height)
        return [
            MetalVertex(position: .init(0, 0), textureCoordinate: .init(0, 0)),
            MetalVertex(position: .init(0, y), textureCoordinate: .init(0, 1)),
            MetalVertex(position: .init(x, 0), textureCoordinate: .init(1, 0)),
            MetalVertex(position: .init(x, 0), textureCoordinate: .init(1, 0)),
            MetalVertex(position: .init(0, y), textureCoordinate: .init(0, 1)),
            MetalVertex(position: .init(x, y), textureCoordinate: .init(1, 1)),
        ]
    }

    private func boundedSlice<Value>(
        _ values: [Value],
        start: Int,
        count: Int
    ) -> ArraySlice<Value> {
        guard start >= 0, count > 0, start < values.count else { return [] }
        return values[start ..< start + min(count, values.count - start)]
    }

    private func encodeVectorSegments(
        _ segments: ArraySlice<CoreSnapshotVectorSegment>,
        opacityMilli: UInt32,
        transform: CoreSnapshotTransform,
        encoder: any MTLRenderCommandEncoder
    ) {
        for segment in segments where segment.flags & 2 != 0 {
            var vertices: [MetalVertex] = []
            let sampleCount = 16
            var previous = devicePoint(segment.p0, transform: transform)
            for index in 1 ... sampleCount {
                let t = Float(index) / Float(sampleCount)
                let point = devicePoint(cubic(segment, t), transform: transform)
                let width = max(
                    0.5,
                    (segment.widthStart + (segment.widthEnd - segment.widthStart) * t)
                        * Float(transform.zoom)
                )
                vertices.append(contentsOf: lineQuad(from: previous, to: point, width: width))
                previous = point
            }
            encodeSolid(
                vertices,
                color: packedColor(segment.colorRGBA, opacityMilli: opacityMilli),
                encoder: encoder
            )
        }
    }

    private func encodeVectorFills(
        _ fills: ArraySlice<CoreSnapshotVectorFill>,
        allSegments: [CoreSnapshotVectorSegment],
        opacityMilli: UInt32,
        transform: CoreSnapshotTransform,
        encoder: any MTLRenderCommandEncoder,
        stencilPipeline: any MTLRenderPipelineState,
        stencilFillPipeline: any MTLRenderPipelineState,
        stencilInvertState: any MTLDepthStencilState,
        stencilFillState: any MTLDepthStencilState,
        viewport: inout SIMD2<Float>
    ) {
        for fill in fills {
            let boundaries = fill.boundaryPathIDs.compactMap { pathID -> [SIMD2<Float>]? in
                let pathSegments = allSegments.filter { $0.pathID == pathID }
                guard let first = pathSegments.first else { return nil }
                var points = [devicePoint(first.p0, transform: transform)]
                for segment in pathSegments {
                    for index in 1 ... 16 {
                        points.append(devicePoint(
                            cubic(segment, Float(index) / 16),
                            transform: transform
                        ))
                    }
                }
                return points.count >= 3 && points.allSatisfy({ $0.x.isFinite && $0.y.isFinite })
                    ? points : nil
            }
            let points = boundaries.flatMap { $0 }
            guard boundaries.count == fill.boundaryPathIDs.count,
                  let first = points.first
            else { continue }
            let bounds = points.dropFirst().reduce(
                (minimum: first, maximum: first)
            ) { partial, point in
                (
                    minimum: SIMD2(
                        min(partial.minimum.x, point.x),
                        min(partial.minimum.y, point.y)
                    ),
                    maximum: SIMD2(
                        max(partial.maximum.x, point.x),
                        max(partial.maximum.y, point.y)
                    )
                )
            }
            let span = max(
                bounds.maximum.x - bounds.minimum.x,
                bounds.maximum.y - bounds.minimum.y,
                1
            )
            let anchor = bounds.minimum - SIMD2<Float>(repeating: span + 1_024)
            guard anchor.x.isFinite, anchor.y.isFinite else { continue }
            var parityTriangles: [MetalVertex] = []
            for boundary in boundaries {
                for index in boundary.indices {
                    let next = boundary.index(after: index) == boundary.endIndex
                        ? boundary.startIndex : boundary.index(after: index)
                    parityTriangles.append(contentsOf: [
                        solidVertex(anchor),
                        solidVertex(boundary[index]),
                        solidVertex(boundary[next]),
                    ])
                }
            }
            guard !parityTriangles.isEmpty else { continue }

            configure(encoder, pipeline: stencilPipeline, viewport: &viewport, sampler: nil)
            encoder.setDepthStencilState(stencilInvertState)
            encoder.setStencilReferenceValue(0)
            encodeSolid(parityTriangles, color: .zero, encoder: encoder)

            configure(encoder, pipeline: stencilFillPipeline, viewport: &viewport, sampler: nil)
            encoder.setDepthStencilState(stencilFillState)
            let fillBounds = quadVertices(from: bounds.minimum, to: bounds.maximum)
            encodeSolid(
                fillBounds,
                color: packedColor(fill.colorRGBA, opacityMilli: opacityMilli),
                encoder: encoder
            )

            configure(encoder, pipeline: stencilPipeline, viewport: &viewport, sampler: nil)
            encoder.setDepthStencilState(stencilInvertState)
            encodeSolid(parityTriangles, color: .zero, encoder: encoder)
            encoder.setDepthStencilState(nil)
        }
    }

    private func encodeAnnotations(
        _ annotations: ArraySlice<CoreSnapshotAnnotationValue>,
        opacityMilli: UInt32,
        transform: CoreSnapshotTransform,
        encoder: any MTLRenderCommandEncoder,
        solidPipeline: any MTLRenderPipelineState,
        opacityPipeline: any MTLRenderPipelineState,
        viewport: inout SIMD2<Float>,
        sampler: (any MTLSamplerState)?
    ) {
        for annotation in annotations {
            if annotation.kind == 1,
               let texture = annotationTexture(annotation, transform: transform)
            {
                configure(
                    encoder,
                    pipeline: opacityPipeline,
                    viewport: &viewport,
                    sampler: sampler
                )
                var opacity = Float(opacityMilli) / 1_000
                encoder.setFragmentBytes(
                    &opacity,
                    length: MemoryLayout<Float>.stride,
                    index: 0
                )
                let p0 = devicePoint(
                    SIMD2(Float(annotation.bounds.x), Float(annotation.bounds.y)),
                    transform: transform
                )
                let p1 = devicePoint(
                    SIMD2(
                        Float(annotation.bounds.x + annotation.bounds.width),
                        Float(annotation.bounds.y + annotation.bounds.height)
                    ),
                    transform: transform
                )
                var vertices = quadVertices(from: p0, to: p1)
                encodeTexturedQuad(encoder, vertices: &vertices, texture: texture)
            } else if annotation.points.count >= 2 {
                configure(
                    encoder,
                    pipeline: solidPipeline,
                    viewport: &viewport,
                    sampler: nil
                )
                let width = max(
                    0.5,
                    Float(annotation.strokeWidthMilli) / 1_000 * Float(transform.zoom)
                )
                var vertices: [MetalVertex] = []
                for index in 1 ..< annotation.points.count {
                    let first = annotation.points[index - 1]
                    let second = annotation.points[index]
                    let p0 = devicePoint(
                        SIMD2(Float(first.xMilli) / 1_000, Float(first.yMilli) / 1_000),
                        transform: transform
                    )
                    let p1 = devicePoint(
                        SIMD2(Float(second.xMilli) / 1_000, Float(second.yMilli) / 1_000),
                        transform: transform
                    )
                    vertices.append(contentsOf: lineQuad(from: p0, to: p1, width: width))
                }
                encodeSolid(
                    vertices,
                    color: coreColor(annotation.color, opacityMilli: opacityMilli),
                    encoder: encoder
                )
            }
        }
    }

    private func encodeGuides(
        _ snapshot: BorrowedCoreSnapshotView,
        transform: CoreSnapshotTransform,
        encoder: any MTLRenderCommandEncoder
    ) {
        for frame in snapshot.shootingFrames where frame.visible && frame.corners.count == 4 {
            var vertices: [MetalVertex] = []
            for index in frame.corners.indices {
                let next = (index + 1) % frame.corners.count
                vertices.append(contentsOf: lineQuad(
                    from: devicePoint(frame.corners[index], transform: transform),
                    to: devicePoint(frame.corners[next], transform: transform),
                    width: 1.5
                ))
            }
            encodeSolid(
                vertices,
                color: SIMD4(0.1, 0.65, 1, 0.85),
                encoder: encoder
            )
        }
        for guide in snapshot.radialGuides {
            encodeSolid(
                lineQuad(
                    from: devicePoint(guide.start, transform: transform),
                    to: devicePoint(guide.end, transform: transform),
                    width: 1
                ),
                color: coreColor(guide.color, opacityMilli: guide.opacityMilli),
                encoder: encoder
            )
        }
        for point in snapshot.vanishingPoints where point.visible {
            let center = devicePoint(point.position, transform: transform)
            let size: Float = 7
            let vertices = lineQuad(
                from: center - SIMD2(size, 0),
                to: center + SIMD2(size, 0),
                width: 1.5
            ) + lineQuad(
                from: center - SIMD2(0, size),
                to: center + SIMD2(0, size),
                width: 1.5
            )
            encodeSolid(
                vertices,
                color: coreColor(point.color, opacityMilli: point.opacityMilli),
                encoder: encoder
            )
        }
        if snapshot.vectorDiagnosticFlags & 8 != 0 {
            for endpoint in snapshot.vectorEndpoints {
                let center = devicePoint(endpoint.point, transform: transform)
                let r: Float = 4
                let vertices = quadVertices(
                    from: center - SIMD2(repeating: r),
                    to: center + SIMD2(repeating: r)
                )
                encodeSolid(vertices, color: SIMD4(1, 0.2, 0.2, 0.9), encoder: encoder)
            }
        }
        if snapshot.vectorDiagnosticFlags & 2 != 0 {
            for segment in snapshot.vectorSegments {
                var vertices: [MetalVertex] = []
                var previous = devicePoint(segment.p0, transform: transform)
                for index in 1 ... 12 {
                    let point = devicePoint(cubic(segment, Float(index) / 12), transform: transform)
                    vertices.append(contentsOf: lineQuad(from: previous, to: point, width: 1))
                    previous = point
                }
                encodeSolid(vertices, color: SIMD4(0, 0.8, 1, 0.8), encoder: encoder)
            }
        }
    }

    private func annotationTexture(
        _ annotation: CoreSnapshotAnnotationValue,
        transform: CoreSnapshotTransform
    ) -> (any MTLTexture)? {
        let width = min(4_096, max(1, Int(Double(annotation.bounds.width) * transform.zoom)))
        let height = min(4_096, max(1, Int(Double(annotation.bounds.height) * transform.zoom)))
        let bytesPerRow = width * 4
        var bytes = [UInt8](repeating: 0, count: bytesPerRow * height)
        let rendered = bytes.withUnsafeMutableBytes { buffer -> Bool in
            guard let base = buffer.baseAddress,
                  let context = CGContext(
                    data: base,
                    width: width,
                    height: height,
                    bitsPerComponent: 8,
                    bytesPerRow: bytesPerRow,
                    space: CGColorSpaceCreateDeviceRGB(),
                    bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
                        | CGBitmapInfo.byteOrder32Little.rawValue
                  )
            else { return false }
            let pointSize = max(1, CGFloat(annotation.fontSizeMilli) / 1_000
                * CGFloat(transform.zoom))
            let font: CTFont = annotation.fontFamily.isEmpty
                ? (CTFontCreateUIFontForLanguage(.system, pointSize, nil)
                    ?? CTFontCreateWithName(".AppleSystemUIFont" as CFString, pointSize, nil))
                : CTFontCreateWithName(annotation.fontFamily as CFString, pointSize, nil)
            let color = cgColor(annotation.color)
            let attributes: [NSAttributedString.Key: Any] = [
                NSAttributedString.Key(kCTFontAttributeName as String): font,
                NSAttributedString.Key(kCTForegroundColorAttributeName as String): color,
            ]
            let line = CTLineCreateWithAttributedString(
                NSAttributedString(string: annotation.text, attributes: attributes)
            )
            context.textPosition = CGPoint(x: 0, y: max(0, CGFloat(height) - pointSize))
            CTLineDraw(line, context)
            return true
        }
        guard rendered else { return nil }
        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .bgra8Unorm_srgb,
            width: width,
            height: height,
            mipmapped: false
        )
        descriptor.storageMode = .managed
        descriptor.usage = .shaderRead
        guard let texture = device.makeTexture(descriptor: descriptor) else { return nil }
        bytes.withUnsafeBytes { buffer in
            texture.replace(
                region: MTLRegionMake2D(0, 0, width, height),
                mipmapLevel: 0,
                withBytes: buffer.baseAddress!,
                bytesPerRow: bytesPerRow
            )
        }
        return texture
    }

    private func cubic(_ segment: CoreSnapshotVectorSegment, _ t: Float) -> SIMD2<Float> {
        let u = 1 - t
        return u * u * u * segment.p0 + 3 * u * u * t * segment.p1
            + 3 * u * t * t * segment.p2 + t * t * t * segment.p3
    }

    private func devicePoint(
        _ point: SIMD2<Float>,
        transform: CoreSnapshotTransform
    ) -> SIMD2<Float> {
        var x = point.x
        var y = point.y
        if transform.flags & 1 != 0 { x = Float(transform.documentWidth) - x }
        if transform.flags & 2 != 0 { y = Float(transform.documentHeight) - y }
        return SIMD2(
            Float(Double(x) * transform.zoom + transform.panX),
            Float(Double(y) * transform.zoom + transform.panY)
        )
    }

    private func lineQuad(
        from: SIMD2<Float>,
        to: SIMD2<Float>,
        width: Float
    ) -> [MetalVertex] {
        let delta = to - from
        let length = max(simd_length(delta), 0.0001)
        let normal = SIMD2(-delta.y, delta.x) / length * (width / 2)
        let a = from + normal
        let b = from - normal
        let c = to + normal
        let d = to - normal
        return [solidVertex(a), solidVertex(b), solidVertex(c),
                solidVertex(c), solidVertex(b), solidVertex(d)]
    }

    private func quadVertices(from: SIMD2<Float>, to: SIMD2<Float>) -> [MetalVertex] {
        let x0 = min(from.x, to.x)
        let y0 = min(from.y, to.y)
        let x1 = max(from.x, to.x)
        let y1 = max(from.y, to.y)
        return [
            MetalVertex(position: SIMD2(x0, y0), textureCoordinate: SIMD2(0, 0)),
            MetalVertex(position: SIMD2(x0, y1), textureCoordinate: SIMD2(0, 1)),
            MetalVertex(position: SIMD2(x1, y0), textureCoordinate: SIMD2(1, 0)),
            MetalVertex(position: SIMD2(x1, y0), textureCoordinate: SIMD2(1, 0)),
            MetalVertex(position: SIMD2(x0, y1), textureCoordinate: SIMD2(0, 1)),
            MetalVertex(position: SIMD2(x1, y1), textureCoordinate: SIMD2(1, 1)),
        ]
    }

    private func solidVertex(_ point: SIMD2<Float>) -> MetalVertex {
        MetalVertex(position: point, textureCoordinate: .zero)
    }

    private func encodeSolid(
        _ vertices: [MetalVertex],
        color: SIMD4<Float>,
        encoder: any MTLRenderCommandEncoder
    ) {
        guard !vertices.isEmpty else { return }
        var mutableVertices = vertices
        var mutableColor = color
        encoder.setVertexBytes(
            &mutableVertices,
            length: MemoryLayout<MetalVertex>.stride * mutableVertices.count,
            index: 0
        )
        encoder.setFragmentBytes(
            &mutableColor,
            length: MemoryLayout<SIMD4<Float>>.stride,
            index: 0
        )
        encoder.drawPrimitives(
            type: .triangle,
            vertexStart: 0,
            vertexCount: mutableVertices.count
        )
    }

    private func packedColor(_ rgba: UInt32, opacityMilli: UInt32) -> SIMD4<Float> {
        let alpha = Float(rgba & 0xff) / 255 * Float(opacityMilli) / 1_000
        return SIMD4(
            Float((rgba >> 24) & 0xff) / 255 * alpha,
            Float((rgba >> 16) & 0xff) / 255 * alpha,
            Float((rgba >> 8) & 0xff) / 255 * alpha,
            alpha
        )
    }

    private func coreColor(_ value: CoreColorValue, opacityMilli: UInt32) -> SIMD4<Float> {
        let maximum: Float = switch value.depth {
        case .binary: 1
        case .grayscale8, .rgba8: 255
        case .grayscale16, .rgba16: 65_535
        }
        let alpha = Float(value.alpha) / maximum * Float(opacityMilli) / 1_000
        return SIMD4(
            Float(value.red) / maximum * alpha,
            Float(value.green) / maximum * alpha,
            Float(value.blue) / maximum * alpha,
            alpha
        )
    }

    private func cgColor(_ value: CoreColorValue) -> CGColor {
        let color = coreColor(value, opacityMilli: 1_000)
        let alpha = max(color.w, 0.0001)
        return CGColor(
            colorSpace: CGColorSpaceCreateDeviceRGB(),
            components: [CGFloat(color.x / alpha), CGFloat(color.y / alpha),
                         CGFloat(color.z / alpha), CGFloat(color.w)]
        ) ?? CGColor(gray: 0, alpha: 1)
    }

    private func validDrawableSize(_ size: CGSize) -> Bool {
        size.width.isFinite && size.height.isFinite && size.width > 0 && size.height > 0
            && size.width <= 32_768 && size.height <= 32_768
    }
}
