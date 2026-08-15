import AppKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

@MainActor
final class ClipboardBroker {
    static let privateType = NSPasteboard.PasteboardType("com.inkpod.typed-clipboard-v1")

    private struct Ticket: Codable {
        let version: UInt32
        let process: UUID
        let clipboard: UInt64
    }

    private let coreHost: CoreHost
    private let pasteboard: NSPasteboard
    private let processID = UUID()
    private var ownedClipboard: CoreClipboardID?
    private var ownedProjection: CoreClipboardProjection?

    init(coreHost: CoreHost, pasteboard: NSPasteboard = .general) {
        self.coreHost = coreHost
        self.pasteboard = pasteboard
    }

    func publish(_ projection: CoreClipboardProjection) async -> Bool {
        guard let png = imageData(projection.raster, type: .png),
              let tiff = imageData(projection.raster, type: .tiff),
              let ticket = try? PropertyListEncoder().encode(Ticket(
                  version: 1,
                  process: processID,
                  clipboard: projection.id.rawValue
              ))
        else {
            _ = await coreHost.releaseClipboard(projection.id).value()
            return false
        }
        let item = NSPasteboardItem()
        item.setData(ticket, forType: Self.privateType)
        item.setData(png, forType: .png)
        item.setData(tiff, forType: .tiff)
        pasteboard.clearContents()
        guard pasteboard.writeObjects([item]) else {
            _ = await coreHost.releaseClipboard(projection.id).value()
            return false
        }
        if let previous = ownedClipboard, previous != projection.id {
            _ = await coreHost.releaseClipboard(previous).value()
        }
        ownedClipboard = projection.id
        ownedProjection = projection
        return true
    }

    func clipboardForPaste() async -> CoreClipboardID? {
        if let data = pasteboard.data(forType: Self.privateType),
           let ticket = try? PropertyListDecoder().decode(Ticket.self, from: data),
           ticket.version == 1,
           ticket.process == processID,
           let ownedClipboard,
           ticket.clipboard == ownedClipboard.rawValue
        {
            return ownedClipboard
        }
        let data = pasteboard.data(forType: .png) ?? pasteboard.data(forType: .tiff)
        guard let data, let raster = decodeStandardImage(data) else { return nil }
        guard case let .clipboardCopied(projection) = await coreHost
            .createClipboard(from: raster).value()
        else {
            return nil
        }
        if let previous = ownedClipboard, previous != projection.id {
            _ = await coreHost.releaseClipboard(previous).value()
        }
        ownedClipboard = projection.id
        ownedProjection = projection
        return projection.id
    }

    func projectionForPaste() async -> CoreClipboardProjection? {
        guard let id = await clipboardForPaste(),
              let ownedProjection,
              ownedProjection.id == id
        else {
            return nil
        }
        return ownedProjection
    }

    func hasPasteableRepresentation() -> Bool {
        pasteboard.availableType(from: [Self.privateType, .png, .tiff]) != nil
    }

    func shutdown() async {
        guard let ownedClipboard else { return }
        self.ownedClipboard = nil
        ownedProjection = nil
        _ = await coreHost.releaseClipboard(ownedClipboard).value()
    }

    private func imageData(
        _ raster: CoreClipboardRaster,
        type: UTType
    ) -> Data? {
        guard raster.isValid,
              let provider = CGDataProvider(data: Data(raster.rgba8) as CFData),
              let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
              let image = CGImage(
                  width: Int(raster.width),
                  height: Int(raster.height),
                  bitsPerComponent: 8,
                  bitsPerPixel: 32,
                  bytesPerRow: Int(raster.rowStrideBytes),
                  space: colorSpace,
                  bitmapInfo: CGBitmapInfo(
                      rawValue: CGImageAlphaInfo.last.rawValue
                          | CGBitmapInfo.byteOrder32Big.rawValue
                  ),
                  provider: provider,
                  decode: nil,
                  shouldInterpolate: false,
                  intent: .defaultIntent
              )
        else {
            return nil
        }
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            data,
            type.identifier as CFString,
            1,
            nil
        ) else {
            return nil
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { return nil }
        return data as Data
    }

    private func decodeStandardImage(_ data: Data) -> CoreClipboardRaster? {
        guard let imageSource = CGImageSourceCreateWithData(data as CFData, nil),
              CGImageSourceGetCount(imageSource) == 1,
              let source = CGImageSourceCreateImageAtIndex(imageSource, 0, nil),
              source.width > 0,
              source.height > 0,
              source.width <= 16_384,
              source.height <= 16_384,
              let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)
        else {
            return nil
        }
        let pixelCount = source.width.multipliedReportingOverflow(by: source.height)
        guard !pixelCount.overflow, pixelCount.partialValue <= 16_777_216 else { return nil }
        let stride = source.width * 4
        var pixels = [UInt8](repeating: 0, count: stride * source.height)
        let rendered = pixels.withUnsafeMutableBytes { bytes -> Bool in
            guard let context = CGContext(
                data: bytes.baseAddress,
                width: source.width,
                height: source.height,
                bitsPerComponent: 8,
                bytesPerRow: stride,
                space: colorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
                    | CGBitmapInfo.byteOrder32Big.rawValue
            ) else {
                return false
            }
            context.interpolationQuality = .none
            context.draw(source, in: CGRect(x: 0, y: 0, width: source.width, height: source.height))
            return true
        }
        guard rendered else { return nil }
        for offset in Swift.stride(from: 0, to: pixels.count, by: 4) {
            let alpha = UInt32(pixels[offset + 3])
            guard alpha > 0, alpha < 255 else { continue }
            for channel in 0 ..< 3 {
                pixels[offset + channel] = UInt8(min(
                    255,
                    (UInt32(pixels[offset + channel]) * 255 + alpha / 2) / alpha
                ))
            }
        }
        return CoreClipboardRaster(
            originX: 0,
            originY: 0,
            width: UInt32(source.width),
            height: UInt32(source.height),
            rowStrideBytes: UInt64(stride),
            rgba8: pixels
        )
    }
}
