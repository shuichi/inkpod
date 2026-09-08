#pragma once

#include <array>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <future>
#include <source_location>

namespace inkpod::tests {

// Only the test's main thread records observations. Fixed storage avoids both
// allocation and console/file I/O between synchronization and timing checks.
// Output is deferred until RunCoreHostTests has released its workers/resources.
class CoreHostDiagnostics final {
public:
    using Clock = std::chrono::steady_clock;

    void Begin() noexcept { started_ = Clock::now(); }

    template<class Evaluate>
    bool FailureCondition(Evaluate&& evaluate,
        std::source_location where = std::source_location::current()) {
        const auto start = Clock::now();
        const bool failed = evaluate();
        Add("failure-condition", where, Clock::now() - start, -1,
            "false", failed ? "true" : "false", !failed);
        return failed;
    }

    template<class Future, class Rep, class Period>
    bool WaitMatches(Future& future, std::chrono::duration<Rep, Period> timeout,
        std::future_status expected,
        std::source_location where = std::source_location::current()) {
        const auto start = Clock::now();
        const auto actual = future.wait_for(timeout);
        Add("future-wait", where, Clock::now() - start,
            std::chrono::duration_cast<std::chrono::microseconds>(timeout).count(),
            StatusName(expected), StatusName(actual), actual == expected);
        return actual == expected;
    }

    void Timing(const char* name, Clock::duration elapsed, std::int64_t limit_us,
        bool passed, std::source_location where = std::source_location::current()) noexcept {
        Add(name, where, elapsed, limit_us, "within-limit",
            passed ? "within-limit" : "limit-exceeded", passed);
    }

    void Poll(const char* name, Clock::time_point start, std::uint64_t attempts,
        bool reached, std::uint64_t observed,
        std::source_location where = std::source_location::current()) noexcept {
        Add(name, where, Clock::now() - start, -1, "reached",
            reached ? "reached" : "not-reached", reached, attempts, observed);
    }

    bool MinimumQueueWait(std::uint64_t observed, std::uint64_t minimum,
        std::source_location where = std::source_location::current()) noexcept {
        const bool passed = observed >= minimum;
        Add("queue-wait-counter", where, Clock::duration::zero(),
            static_cast<std::int64_t>(minimum), "at-least-limit",
            passed ? "at-least-limit" : "below-limit", passed, 0U, observed);
        return passed;
    }

    int Exit(int code,
        std::source_location where = std::source_location::current()) noexcept {
        exit_location_ = where;
        return code;
    }

    void Print(int code) const noexcept {
        const auto total_us = std::chrono::duration_cast<std::chrono::microseconds>(
            Clock::now() - started_).count();
        for (std::size_t index = 0; index < count_; ++index) {
            const auto& entry = records_[index];
            std::fprintf(stderr,
                "core-host-check kind=%s function=\"%s\" line=%u elapsed_us=%lld "
                "limit_us=%lld expected=%s actual=%s passed=%u attempts=%llu observed=%llu\n",
                entry.name, entry.where.function_name(), entry.where.line(),
                static_cast<long long>(entry.elapsed_us),
                static_cast<long long>(entry.limit_us), entry.expected, entry.actual,
                entry.passed ? 1U : 0U,
                static_cast<unsigned long long>(entry.attempts),
                static_cast<unsigned long long>(entry.observed));
        }
        std::fprintf(stderr,
            "core-host-result exit_code=%d line=%u elapsed_us=%lld records=%zu overflow=%u\n",
            code, exit_location_.line(), static_cast<long long>(total_us),
            count_, overflow_ ? 1U : 0U);
    }

private:
    struct Record {
        const char* name{};
        std::source_location where{};
        std::int64_t elapsed_us{};
        std::int64_t limit_us{};
        const char* expected{};
        const char* actual{};
        bool passed{};
        std::uint64_t attempts{};
        std::uint64_t observed{};
    };

    void Add(const char* name, std::source_location where, Clock::duration elapsed,
        std::int64_t limit_us, const char* expected, const char* actual, bool passed,
        std::uint64_t attempts = 0U, std::uint64_t observed = 0U) noexcept {
        if (count_ == records_.size()) {
            overflow_ = true;
            return;
        }
        records_[count_++] = Record{name, where,
            std::chrono::duration_cast<std::chrono::microseconds>(elapsed).count(),
            limit_us, expected, actual, passed, attempts, observed};
    }

    static const char* StatusName(std::future_status value) noexcept {
        switch (value) {
        case std::future_status::ready: return "ready";
        case std::future_status::timeout: return "timeout";
        case std::future_status::deferred: return "deferred";
        }
        return "unknown";
    }

    std::array<Record, 512> records_{};
    std::size_t count_{};
    bool overflow_{};
    Clock::time_point started_{};
    std::source_location exit_location_{};
};

inline CoreHostDiagnostics core_host_diagnostics;

}  // namespace inkpod::tests
