import Foundation
import InkpodCoreC

public struct CoreSnapshotEnvelope: @unchecked Sendable, Equatable {
    public let route: CoreSnapshotRoute
    public let documentRevision: UInt64
    public let viewRevision: UInt64
    let owner: CoreOwnedSnapshot

    init(
        route: CoreSnapshotRoute,
        documentRevision: UInt64,
        viewRevision: UInt64,
        owner: CoreOwnedSnapshot
    ) {
        self.route = route
        self.documentRevision = documentRevision
        self.viewRevision = viewRevision
        self.owner = owner
    }

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.route == rhs.route && lhs.documentRevision == rhs.documentRevision
            && lhs.viewRevision == rhs.viewRevision && lhs.owner === rhs.owner
    }
}

struct CoreSnapshotTransform: Equatable, Sendable {
    let flags: UInt32
    let viewRevision: UInt64
    let zoom: Double
    let panX: Double
    let panY: Double
    let documentWidth: UInt32
    let documentHeight: UInt32
}

struct BorrowedCoreSnapshotTile {
    let pixelFormat: UInt32
    let tileID: UInt64
    let originX: Int32
    let originY: Int32
    let width: UInt32
    let height: UInt32
    let strideBytes: UInt32
    let pixels: UnsafeRawBufferPointer
    let tileRevision: UInt64
}

struct CoreSnapshotVectorSegment {
    let flags: UInt32
    let pathID: UInt64
    let planeID: UInt64
    let colorRGBA: UInt32
    let p0: SIMD2<Float>
    let p1: SIMD2<Float>
    let p2: SIMD2<Float>
    let p3: SIMD2<Float>
    let widthStart: Float
    let widthEnd: Float
}

struct CoreSnapshotVectorFill {
    let fillID: UInt64
    let planeID: UInt64
    let colorRGBA: UInt32
    let boundaryPathIDs: [UInt64]
}

struct CoreSnapshotAnnotationValue {
    let kind: UInt32
    let objectID: UInt64
    let layerID: UInt64
    let output: UInt32
    let styleFlags: UInt32
    let bounds: CoreFrameRect
    let fontSizeMilli: UInt32
    let strokeWidthMilli: UInt32
    let color: CoreColorValue
    let fontFamily: String
    let text: String
    let points: [CoreAnnotationPoint]
}

struct CoreSnapshotShootingFrameValue {
    let id: UInt64
    let visible: Bool
    let corners: [SIMD2<Float>]
}

struct CoreSnapshotVanishingPointValue {
    let id: UInt64
    let visible: Bool
    let position: SIMD2<Float>
    let color: CoreColorValue
    let opacityMilli: UInt32
}

struct CoreSnapshotRadialGuideValue {
    let pointID: UInt64
    let start: SIMD2<Float>
    let end: SIMD2<Float>
    let color: CoreColorValue
    let opacityMilli: UInt32
}

struct CoreSnapshotVectorEndpointValue {
    let pathID: UInt64
    let planeID: UInt64
    let point: SIMD2<Float>
}

struct CoreSnapshotRenderPassValue {
    let kind: UInt32
    let layerID: UInt64
    let planeID: UInt64
    let opacityMilli: UInt32
    let firstItem: Int
    let itemCount: Int
}

struct BorrowedCoreSnapshotView {
    let revision: UInt64
    let featureFlags: UInt64
    let transform: CoreSnapshotTransform
    let tiles: [BorrowedCoreSnapshotTile]
    let vectorSegments: [CoreSnapshotVectorSegment]
    let vectorFills: [CoreSnapshotVectorFill]
    let annotations: [CoreSnapshotAnnotationValue]
    let shootingFrames: [CoreSnapshotShootingFrameValue]
    let vanishingPoints: [CoreSnapshotVanishingPointValue]
    let radialGuides: [CoreSnapshotRadialGuideValue]
    let vectorDiagnosticFlags: UInt32
    let vectorEndpoints: [CoreSnapshotVectorEndpointValue]
    let renderPasses: [CoreSnapshotRenderPassValue]
    let adjustmentLUTs: [[UInt8]]
}

enum CoreSnapshotReadError: Error, Equatable {
    case released
    case invalidView(CoreStatus)
    case invalidTransform(CoreStatus)
    case unsupportedPixelFormat
    case invalidTileLayout
    case invalidM8Layout
}

final class CoreOwnedSnapshot: @unchecked Sendable {
    private let lock = NSLock()
    private var raw: OpaquePointer?
    private var releaseCalls = 0

    init(raw: OpaquePointer) {
        self.raw = raw
    }

    var ffiReleaseCount: Int {
        lock.withLock { releaseCalls }
    }

    func withBorrowedRenderView<T>(
        _ body: (BorrowedCoreSnapshotView) throws -> T
    ) throws -> T {
        try lock.withLock {
            guard let raw else {
                throw CoreSnapshotReadError.released
            }

            var view = InkpodSnapshotView()
            view.struct_size = UInt32(MemoryLayout<InkpodSnapshotView>.size)
            let viewStatus = CoreStatus(cValue: inkpod_snapshot_get_view(raw, &view))
            guard viewStatus == .ok else {
                throw CoreSnapshotReadError.invalidView(viewStatus)
            }
            var transform = InkpodSnapshotTransform()
            transform.struct_size = UInt32(MemoryLayout<InkpodSnapshotTransform>.size)
            let transformStatus = CoreStatus(
                cValue: inkpod_snapshot_get_transform(raw, &transform)
            )
            guard transformStatus == .ok else {
                throw CoreSnapshotReadError.invalidTransform(transformStatus)
            }
            guard view.tile_stride_bytes == UInt64(MemoryLayout<InkpodSnapshotTile>.stride),
                  view.tile_count <= 262_144,
                  view.tile_count == 0 || view.tiles != nil
            else {
                throw CoreSnapshotReadError.invalidTileLayout
            }

            var tiles: [BorrowedCoreSnapshotTile] = []
            tiles.reserveCapacity(Int(view.tile_count))
            for index in 0 ..< Int(view.tile_count) {
                let tile = view.tiles!.advanced(by: index).pointee
                guard tile.struct_size == UInt32(MemoryLayout<InkpodSnapshotTile>.size),
                      tile.pixel_format == inkpod_bridge_pixel_premultiplied_bgra8(),
                      tile.width > 0,
                      tile.height > 0,
                      tile.origin_x >= 0,
                      tile.origin_y >= 0,
                      tile.width <= UInt32(Int32.max),
                      tile.height <= UInt32(Int32.max),
                      Int64(tile.origin_x) + Int64(tile.width) <= Int64(Int32.max),
                      Int64(tile.origin_y) + Int64(tile.height) <= Int64(Int32.max),
                      Int64(tile.origin_x) + Int64(tile.width)
                          <= Int64(transform.document_width),
                      Int64(tile.origin_y) + Int64(tile.height)
                          <= Int64(transform.document_height),
                      UInt64(tile.stride_bytes) >= UInt64(tile.width) * 4,
                      tile.pixel_bytes == UInt64(tile.stride_bytes) * UInt64(tile.height),
                      tile.pixel_bytes <= UInt64(Int.max),
                      tile.pixels != nil
                else {
                    throw tile.pixel_format == inkpod_bridge_pixel_premultiplied_bgra8()
                        ? CoreSnapshotReadError.invalidTileLayout
                        : CoreSnapshotReadError.unsupportedPixelFormat
                }
                tiles.append(
                    BorrowedCoreSnapshotTile(
                        pixelFormat: tile.pixel_format,
                        tileID: tile.tile_id,
                        originX: tile.origin_x,
                        originY: tile.origin_y,
                        width: tile.width,
                        height: tile.height,
                        strideBytes: tile.stride_bytes,
                        pixels: UnsafeRawBufferPointer(
                            start: tile.pixels,
                            count: Int(tile.pixel_bytes)
                        ),
                        tileRevision: tile.tile_revision
                    )
                )
            }

            var vectors = InkpodSnapshotVectorView()
            vectors.struct_size = UInt32(MemoryLayout<InkpodSnapshotVectorView>.size)
            var annotations = InkpodSnapshotAnnotationView()
            annotations.struct_size = UInt32(MemoryLayout<InkpodSnapshotAnnotationView>.size)
            var frames = InkpodSnapshotShootingFrameView()
            frames.struct_size = UInt32(MemoryLayout<InkpodSnapshotShootingFrameView>.size)
            var vanishing = InkpodSnapshotVanishingPointView()
            vanishing.struct_size = UInt32(MemoryLayout<InkpodSnapshotVanishingPointView>.size)
            var diagnostics = InkpodSnapshotVectorDiagnostics()
            diagnostics.struct_size = UInt32(MemoryLayout<InkpodSnapshotVectorDiagnostics>.size)
            var renderPlan = InkpodSnapshotRenderPlan()
            renderPlan.struct_size = UInt32(MemoryLayout<InkpodSnapshotRenderPlan>.size)
            guard CoreStatus(cValue: inkpod_snapshot_get_vectors(raw, &vectors)) == .ok,
                  CoreStatus(cValue: inkpod_snapshot_get_annotations(raw, &annotations)) == .ok,
                  CoreStatus(cValue: inkpod_snapshot_get_shooting_frames(raw, &frames)) == .ok,
                  CoreStatus(cValue: inkpod_snapshot_get_vanishing_points(raw, &vanishing)) == .ok,
                  CoreStatus(cValue: inkpod_snapshot_get_vector_diagnostics(raw, &diagnostics)) == .ok,
                  CoreStatus(cValue: inkpod_snapshot_get_render_plan(raw, &renderPlan)) == .ok,
                  validSpan(vectors.segments, vectors.segment_count, vectors.segment_stride_bytes,
                            InkpodSnapshotVectorSegment.self, maximum: 1_000_000),
                  validSpan(vectors.fills, vectors.fill_count, vectors.fill_stride_bytes,
                            InkpodSnapshotVectorFill.self, maximum: 262_144),
                  vectors.boundary_path_count <= 1_000_000,
                  vectors.boundary_path_count == 0 || vectors.boundary_path_ids != nil,
                  validSpan(annotations.objects, annotations.object_count,
                            annotations.object_stride_bytes, InkpodSnapshotAnnotation.self,
                            maximum: 262_144),
                  annotations.utf8_byte_count <= 64 * 1_024 * 1_024,
                  annotations.utf8_byte_count == 0 || annotations.utf8_bytes != nil,
                  validSpan(annotations.points, annotations.point_count,
                            annotations.point_stride_bytes, InkpodAnnotationPoint.self,
                            maximum: 1_000_000),
                  validSpan(frames.frames, frames.frame_count, frames.frame_stride_bytes,
                            InkpodShootingFrameInfo.self, maximum: 1),
                  validSpan(vanishing.points, vanishing.point_count,
                            vanishing.point_stride_bytes, InkpodVanishingPointInfo.self,
                            maximum: 4_096),
                  validSpan(vanishing.radial_guides, vanishing.radial_guide_count,
                            vanishing.radial_guide_stride_bytes, InkpodSnapshotRadialGuide.self,
                            maximum: 1_000_000),
                  validSpan(diagnostics.endpoints, diagnostics.endpoint_count,
                            diagnostics.endpoint_stride_bytes, InkpodSnapshotVectorEndpoint.self,
                            maximum: 1_000_000),
                  validSpan(renderPlan.passes, renderPlan.pass_count,
                            renderPlan.pass_stride_bytes, InkpodSnapshotRenderPass.self,
                            maximum: 1_000_000),
                  renderPlan.adjustment_lut_count <= 4_096,
                  renderPlan.adjustment_lut_stride_bytes >= 768,
                  renderPlan.adjustment_lut_count == 0
                    || renderPlan.adjustment_luts_rgb8 != nil
            else {
                throw CoreSnapshotReadError.invalidM8Layout
            }

            let boundaryIDs = Array(UnsafeBufferPointer(
                start: vectors.boundary_path_ids,
                count: Int(vectors.boundary_path_count)
            ))
            let vectorSegments = try copyStrided(
                vectors.segments,
                count: vectors.segment_count,
                stride: vectors.segment_stride_bytes,
                as: InkpodSnapshotVectorSegment.self
            ).map { segment in
                guard segment.struct_size == UInt32(MemoryLayout<InkpodSnapshotVectorSegment>.size),
                      segment.width_start.isFinite, segment.width_end.isFinite
                else { throw CoreSnapshotReadError.invalidM8Layout }
                return CoreSnapshotVectorSegment(
                    flags: segment.flags,
                    pathID: segment.path_id,
                    planeID: segment.plane_id,
                    colorRGBA: segment.color_rgba,
                    p0: .init(segment.p0.x, segment.p0.y),
                    p1: .init(segment.p1.x, segment.p1.y),
                    p2: .init(segment.p2.x, segment.p2.y),
                    p3: .init(segment.p3.x, segment.p3.y),
                    widthStart: segment.width_start,
                    widthEnd: segment.width_end
                )
            }
            let vectorFills = try copyStrided(
                vectors.fills,
                count: vectors.fill_count,
                stride: vectors.fill_stride_bytes,
                as: InkpodSnapshotVectorFill.self
            ).map { fill in
                guard fill.struct_size == UInt32(MemoryLayout<InkpodSnapshotVectorFill>.size),
                      fill.first_boundary_path <= UInt64(boundaryIDs.count),
                      fill.boundary_path_count <= UInt64(boundaryIDs.count)
                        - fill.first_boundary_path
                else { throw CoreSnapshotReadError.invalidM8Layout }
                let start = Int(fill.first_boundary_path)
                return CoreSnapshotVectorFill(
                    fillID: fill.fill_id,
                    planeID: fill.plane_id,
                    colorRGBA: fill.color_rgba,
                    boundaryPathIDs: Array(
                        boundaryIDs[start ..< start + Int(fill.boundary_path_count)]
                    )
                )
            }
            let utf8 = UnsafeRawBufferPointer(
                start: annotations.utf8_bytes,
                count: Int(annotations.utf8_byte_count)
            )
            let annotationPoints = try copyStrided(
                annotations.points,
                count: annotations.point_count,
                stride: annotations.point_stride_bytes,
                as: InkpodAnnotationPoint.self
            )
            let annotationValues = try copyStrided(
                annotations.objects,
                count: annotations.object_count,
                stride: annotations.object_stride_bytes,
                as: InkpodSnapshotAnnotation.self
            ).map { object in
                guard object.struct_size == UInt32(MemoryLayout<InkpodSnapshotAnnotation>.size),
                      object.font_utf8_offset <= UInt64(utf8.count),
                      object.font_utf8_bytes <= UInt64(utf8.count) - object.font_utf8_offset,
                      object.text_utf8_offset <= UInt64(utf8.count),
                      object.text_utf8_bytes <= UInt64(utf8.count) - object.text_utf8_offset,
                      object.first_point <= UInt64(annotationPoints.count),
                      object.point_count <= UInt64(annotationPoints.count) - object.first_point,
                      let color = coreColorValue(object.color)
                else { throw CoreSnapshotReadError.invalidM8Layout }
                func string(offset: UInt64, count: UInt64) throws -> String {
                    let bytes = utf8[Int(offset) ..< Int(offset + count)]
                    guard let value = String(bytes: bytes, encoding: .utf8) else {
                        throw CoreSnapshotReadError.invalidM8Layout
                    }
                    return value
                }
                let pointStart = Int(object.first_point)
                return CoreSnapshotAnnotationValue(
                    kind: object.kind,
                    objectID: object.object_id,
                    layerID: object.layer_id,
                    output: object.output,
                    styleFlags: object.style_flags,
                    bounds: CoreFrameRect(
                        x: object.bounds.x,
                        y: object.bounds.y,
                        width: object.bounds.width,
                        height: object.bounds.height
                    ),
                    fontSizeMilli: object.font_size_milli,
                    strokeWidthMilli: object.stroke_width_milli,
                    color: color,
                    fontFamily: try string(
                        offset: object.font_utf8_offset,
                        count: object.font_utf8_bytes
                    ),
                    text: try string(
                        offset: object.text_utf8_offset,
                        count: object.text_utf8_bytes
                    ),
                    points: annotationPoints[
                        pointStart ..< pointStart + Int(object.point_count)
                    ].map { CoreAnnotationPoint(xMilli: $0.x_milli, yMilli: $0.y_milli) }
                )
            }
            let frameValues = try copyStrided(
                frames.frames,
                count: frames.frame_count,
                stride: frames.frame_stride_bytes,
                as: InkpodShootingFrameInfo.self
            ).map { frame in
                let corners = withUnsafeBytes(of: frame.corners) { bytes in
                    bytes.bindMemory(to: InkpodShootingFramePoint.self).prefix(4).map {
                        SIMD2(Float($0.x_milli) / 1_000, Float($0.y_milli) / 1_000)
                    }
                }
                return CoreSnapshotShootingFrameValue(
                    id: frame.frame_id,
                    visible: frame.visible != 0,
                    corners: corners
                )
            }
            let vanishingValues = try copyStrided(
                vanishing.points,
                count: vanishing.point_count,
                stride: vanishing.point_stride_bytes,
                as: InkpodVanishingPointInfo.self
            ).map { point in
                guard let color = coreColorValue(point.color) else {
                    throw CoreSnapshotReadError.invalidM8Layout
                }
                return CoreSnapshotVanishingPointValue(
                    id: point.point_id,
                    visible: point.visible != 0,
                    position: .init(
                        Float(point.x_milli) / 1_000,
                        Float(point.y_milli) / 1_000
                    ),
                    color: color,
                    opacityMilli: point.opacity_milli
                )
            }
            let radialGuides = try copyStrided(
                vanishing.radial_guides,
                count: vanishing.radial_guide_count,
                stride: vanishing.radial_guide_stride_bytes,
                as: InkpodSnapshotRadialGuide.self
            ).map { guide in
                guard let color = coreColorValue(guide.color) else {
                    throw CoreSnapshotReadError.invalidM8Layout
                }
                return CoreSnapshotRadialGuideValue(
                    pointID: guide.point_id,
                    start: .init(
                        Float(guide.start_x_milli) / 1_000,
                        Float(guide.start_y_milli) / 1_000
                    ),
                    end: .init(
                        Float(guide.end_x_milli) / 1_000,
                        Float(guide.end_y_milli) / 1_000
                    ),
                    color: color,
                    opacityMilli: guide.opacity_milli
                )
            }
            let endpoints = try copyStrided(
                diagnostics.endpoints,
                count: diagnostics.endpoint_count,
                stride: diagnostics.endpoint_stride_bytes,
                as: InkpodSnapshotVectorEndpoint.self
            ).map {
                CoreSnapshotVectorEndpointValue(
                    pathID: $0.path_id,
                    planeID: $0.plane_id,
                    point: .init($0.point.x, $0.point.y)
                )
            }
            let renderPasses = try copyStrided(
                renderPlan.passes,
                count: renderPlan.pass_count,
                stride: renderPlan.pass_stride_bytes,
                as: InkpodSnapshotRenderPass.self
            ).map { pass in
                guard pass.first_item <= UInt64(Int.max),
                      pass.item_count <= UInt64(Int.max),
                      pass.first_item <= UInt64.max - pass.item_count
                else { throw CoreSnapshotReadError.invalidM8Layout }
                return CoreSnapshotRenderPassValue(
                    kind: pass.kind,
                    layerID: pass.layer_id,
                    planeID: pass.plane_id,
                    opacityMilli: pass.opacity_milli,
                    firstItem: Int(pass.first_item),
                    itemCount: Int(pass.item_count)
                )
            }
            let adjustmentLUTs: [[UInt8]] = (0 ..< Int(renderPlan.adjustment_lut_count)).map {
                let start = renderPlan.adjustment_luts_rgb8!.advanced(
                    by: $0 * Int(renderPlan.adjustment_lut_stride_bytes)
                )
                return Array(UnsafeBufferPointer(start: start, count: 768))
            }

            return try body(
                BorrowedCoreSnapshotView(
                    revision: view.revision,
                    featureFlags: view.feature_flags,
                    transform: CoreSnapshotTransform(
                        flags: transform.flags,
                        viewRevision: transform.view_revision,
                        zoom: transform.zoom,
                        panX: transform.pan_x,
                        panY: transform.pan_y,
                        documentWidth: transform.document_width,
                        documentHeight: transform.document_height
                    ),
                    tiles: tiles,
                    vectorSegments: vectorSegments,
                    vectorFills: vectorFills,
                    annotations: annotationValues,
                    shootingFrames: frameValues,
                    vanishingPoints: vanishingValues,
                    radialGuides: radialGuides,
                    vectorDiagnosticFlags: diagnostics.flags,
                    vectorEndpoints: endpoints,
                    renderPasses: renderPasses,
                    adjustmentLUTs: adjustmentLUTs
                )
            )
        }
    }

    @discardableResult
    func release() -> CoreStatus {
        lock.withLock {
            guard let raw else {
                return .ok
            }
            var owner: OpaquePointer? = raw
            releaseCalls += 1
            let status = CoreStatus(cValue: inkpod_snapshot_release(&owner))
            self.raw = owner
            return status
        }
    }

    deinit {
        _ = release()
    }
}

private func validSpan<T>(
    _ pointer: UnsafePointer<T>?,
    _ count: UInt64,
    _ stride: UInt64,
    _: T.Type,
    maximum: UInt64
) -> Bool {
    count <= maximum && (count == 0
        ? pointer == nil || stride == 0 || stride >= UInt64(MemoryLayout<T>.size)
        : pointer != nil && stride >= UInt64(MemoryLayout<T>.size)
            && stride % UInt64(MemoryLayout<T>.alignment) == 0)
}

private func copyStrided<T>(
    _ pointer: UnsafePointer<T>?,
    count: UInt64,
    stride: UInt64,
    as: T.Type
) throws -> [T] {
    guard count <= UInt64(Int.max), count == 0 || pointer != nil else {
        throw CoreSnapshotReadError.invalidM8Layout
    }
    guard count > 0 else { return [] }
    let raw = UnsafeRawPointer(pointer!)
    return (0 ..< Int(count)).map { index in
        raw.advanced(by: index * Int(stride)).load(as: T.self)
    }
}

private func coreColorValue(_ input: InkpodColorValue) -> CoreColorValue? {
    guard let depth = CoreColorDepth(rawValue: input.depth) else { return nil }
    let value = CoreColorValue(
        depth: depth,
        red: input.red,
        green: input.green,
        blue: input.blue,
        alpha: input.alpha
    )
    return value.hasValidNativeComponents ? value : nil
}
