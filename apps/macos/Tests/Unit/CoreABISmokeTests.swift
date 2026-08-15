import XCTest
import InkpodCoreBridge

final class CoreABISmokeTests: XCTestCase {
    func testSuccessUsesDedicatedOwnerThreadAndCrossThreadSnapshotRelease() throws {
        let result = try CoreABISmoke.run()

        XCTAssertEqual(result.abiVersion, 15)
        XCTAssertEqual(result.snapshotRevision, 0)
        XCTAssertNotEqual(result.ownerThreadID, result.callerThreadID)
        XCTAssertNotEqual(result.ownerThreadID, result.snapshotReleaseThreadID)
        XCTAssertTrue(result.coreOwnerWasNullAfterDestroy)
        XCTAssertEqual(result.snapshotReleaseCount, 1)
    }

    func testABIMismatchCopiesThreadLocalDiagnosticOnOwnerThread() throws {
        let result = try CoreABISmoke.probeABIMismatch()

        XCTAssertEqual(result.status, .incompatibleABI)
        XCTAssertFalse(result.diagnostic.isEmpty)
        XCTAssertEqual(result.failureThreadID, result.diagnosticThreadID)
    }

    func testShortStructAndNullAreRejectedWithoutPublishingOwners() throws {
        let result = try CoreABISmoke.probeInvalidCreateInputs()

        XCTAssertEqual(result.shortStructStatus, .incompatibleABI)
        XCTAssertEqual(result.nullConfigStatus, .invalidArgument)
        XCTAssertTrue(result.ownerStayedNull)
    }

    func testCoreOperationOnAnotherThreadIsRejectedWithoutMovingOwnership() throws {
        let result = try CoreABISmoke.probeWrongThread()

        XCTAssertEqual(result.status, .wrongThread)
        XCTAssertTrue(result.snapshotOwnerStayedNull)
        XCTAssertTrue(result.ownerThreadCouldDestroy)
    }

    func testUnknownStatusRemainsObservable() {
        XCTAssertEqual(CoreStatus(cValue: UInt32.max), .unknown(UInt32.max))
    }

    func testSnapshotReleaseIsIdempotentAndCallsFFIExactlyOnce() throws {
        let result = try CoreABISmoke.probeDoubleSnapshotRelease()

        XCTAssertEqual(result.first, .ok)
        XCTAssertEqual(result.second, .ok)
        XCTAssertEqual(result.ffiReleaseCount, 1)
    }
}
