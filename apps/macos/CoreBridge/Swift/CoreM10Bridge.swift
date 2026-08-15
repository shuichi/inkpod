import Foundation
import InkpodCoreC

final class CoreBatchFFIStorage {
    private var releases: [() -> Void] = []
    private(set) var input = InkpodBatchGraphInput()

    init(_ graph: CoreBatchGraphDraft) {
        let name = allocateBytes(graph.name)
        let folder = allocateBytes(graph.output.folder)
        let basename = allocateBytes(graph.output.basename)
        let inputs = graph.inputs.map { selector in
            let path = allocateBytes(selector.path)
            var value = InkpodBatchInput()
            value.struct_size = UInt32(MemoryLayout<InkpodBatchInput>.size)
            value.kind = selector.kind.rawValue
            value.path_utf8 = path.pointer
            value.path_bytes = path.count
            value.first_cell = selector.firstCell
            value.last_cell = selector.lastCell
            return value
        }
        let operations = graph.operations.map(makeOperation)
        let inputRecords = allocate(inputs)
        let operationRecords = allocate(operations)

        input.struct_size = UInt32(MemoryLayout<InkpodBatchGraphInput>.size)
        input.version = 2
        input.name_utf8 = name.pointer
        input.name_bytes = name.count
        input.inputs = inputRecords
        input.input_count = UInt64(inputs.count)
        input.input_stride_bytes = UInt64(MemoryLayout<InkpodBatchInput>.stride)
        input.operations = operationRecords
        input.operation_count = UInt64(operations.count)
        input.operation_stride_bytes = UInt64(MemoryLayout<InkpodBatchOperationInput>.stride)
        input.output_policy = graph.output.policy.rawValue
        input.failure_policy = graph.output.failurePolicy.rawValue
        input.output_flags = (graph.output.cellFolder ? UInt64(1) : 0)
            | (graph.output.descending ? UInt64(1 << 1) : 0)
            | (graph.output.previewBeforeSave
                ? UInt64(1 << 2) : 0)
        input.output_folder_utf8 = folder.pointer
        input.output_folder_bytes = folder.count
        input.basename_utf8 = basename.pointer
        input.basename_bytes = basename.count
        input.start_number = graph.output.startNumber
        input.wait_milliseconds = graph.output.waitMilliseconds
    }

    deinit {
        for release in releases.reversed() { release() }
    }

    private func makeOperation(_ operation: CoreBatchOperation) -> InkpodBatchOperationInput {
        var value = InkpodBatchOperationInput()
        value.struct_size = UInt32(MemoryLayout<InkpodBatchOperationInput>.size)
        value.version = 2
        value.kind = operation.kind.rawValue
        value.flags = (operation.enabled ? UInt64(1) : 0)
            | (operation.configureEachRun
                ? UInt64(1 << 1) : 0)
        if let target = operation.target {
            value.layer_id = target.layerID ?? 0
            value.plane_id = target.planeID ?? 0
            value.layer_kind = target.layerKind?.rawValue ?? 0
            value.plane_kind = target.planeKind?.rawValue ?? 0
            value.missing_policy = target.missingPolicy.rawValue
        }
        withUnsafeMutableBytes(of: &value.parameters) { bytes in
            let parameters = bytes.bindMemory(to: Int64.self)
            for (index, parameter) in operation.parameters.enumerated() {
                parameters[index] = parameter
            }
        }
        if operation.kind == .separation,
           let replacement = operation.colorPairs.first?.newColor
        {
            value.color_0 = m10FFIColor(replacement)
        }
        let colors = operation.colors.map(m10FFIColor)
        let colorPointer = allocate(colors)
        value.colors.struct_size = UInt32(MemoryLayout<InkpodColorArray>.size)
        value.colors.colors = colorPointer
        value.colors.color_count = UInt64(colors.count)
        value.colors.color_stride_bytes = colors.isEmpty
            ? 0 : UInt64(MemoryLayout<InkpodColorValue>.stride)

        let pairs = operation.colorPairs.map { pair in
            var record = InkpodBatchColorPairInput()
            record.struct_size = UInt32(MemoryLayout<InkpodBatchColorPairInput>.size)
            record.enabled = pair.enabled ? 1 : 0
            record.old_color = m10FFIColor(pair.oldColor)
            record.new_color = m10FFIColor(pair.newColor)
            return record
        }
        value.color_pairs = allocate(pairs)
        value.color_pair_count = UInt64(pairs.count)
        value.color_pair_stride_bytes = pairs.isEmpty
            ? 0 : UInt64(MemoryLayout<InkpodBatchColorPairInput>.stride)

        let seeds = operation.seeds.map { seed in
            var record = InkpodBatchSeedInput()
            record.struct_size = UInt32(MemoryLayout<InkpodBatchSeedInput>.size)
            record.flags = (seed.enabled ? UInt32(1 << 1) : 0)
                | (seed.expectedColor == nil ? 0 : UInt32(1))
            record.x = seed.x
            record.y = seed.y
            record.tolerance = UInt32(seed.tolerance)
            record.gap_close = UInt32(seed.gapClose)
            record.fill_color = m10FFIColor(seed.fillColor)
            if let expected = seed.expectedColor { record.expected_color = m10FFIColor(expected) }
            return record
        }
        value.seeds = allocate(seeds)
        value.seed_count = UInt64(seeds.count)
        value.seed_stride_bytes = seeds.isEmpty
            ? 0 : UInt64(MemoryLayout<InkpodBatchSeedInput>.stride)

        if let filter = operation.filter {
            let points = filter.curvePoints.map { point in
                var record = InkpodCurvePoint()
                record.struct_size = UInt32(MemoryLayout<InkpodCurvePoint>.size)
                record.input = point.input
                record.output = point.output
                return record
            }
            var record = InkpodFilterInput()
            record.struct_size = UInt32(MemoryLayout<InkpodFilterInput>.size)
            record.kind = filter.kind.rawValue
            record.plane_id = filter.planeID
            record.channel = filter.channel.rawValue
            record.interpolation = filter.interpolation.rawValue
            let parameters = filter.parameters + Array(repeating: 0, count: 5 - filter.parameters.count)
            record.parameter_0 = parameters[0]
            record.parameter_1 = parameters[1]
            record.parameter_2 = parameters[2]
            record.parameter_3 = parameters[3]
            record.parameter_4 = parameters[4]
            record.points = allocate(points)
            record.point_count = UInt64(points.count)
            record.point_stride_bytes = points.isEmpty
                ? 0 : UInt32(MemoryLayout<InkpodCurvePoint>.stride)
            value.filter = allocate([record])
        }
        return value
    }

    private func allocateBytes(_ string: String) -> (pointer: UnsafePointer<UInt8>?, count: UInt64) {
        let bytes = Array(string.utf8)
        return (allocate(bytes), UInt64(bytes.count))
    }

    private func allocate<Element>(_ values: [Element]) -> UnsafePointer<Element>? {
        guard !values.isEmpty else { return nil }
        let pointer = UnsafeMutablePointer<Element>.allocate(capacity: values.count)
        pointer.initialize(from: values, count: values.count)
        releases.append {
            pointer.deinitialize(count: values.count)
            pointer.deallocate()
        }
        return UnsafePointer(pointer)
    }
}

func m10FFIColor(_ color: CoreColorValue) -> InkpodColorValue {
    var output = InkpodColorValue()
    output.struct_size = UInt32(MemoryLayout<InkpodColorValue>.size)
    output.depth = color.depth.rawValue
    output.red = color.red
    output.green = color.green
    output.blue = color.blue
    output.alpha = color.alpha
    return output
}

func m10CoreColor(_ color: InkpodColorValue) -> CoreColorValue? {
    guard let depth = CoreColorDepth(rawValue: color.depth) else { return nil }
    let value = CoreColorValue(
        depth: depth,
        red: color.red,
        green: color.green,
        blue: color.blue,
        alpha: color.alpha
    )
    return value.hasValidNativeComponents ? value : nil
}

func m10String(_ pointer: UnsafePointer<UInt8>?, _ count: UInt64) -> String? {
    guard count <= UInt64(Int.max) else { return nil }
    guard count != 0 else { return "" }
    guard let pointer else { return nil }
    return String(bytes: UnsafeBufferPointer(start: pointer, count: Int(count)), encoding: .utf8)
}
