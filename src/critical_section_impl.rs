use critical_section::RawRestoreState;

struct PowdrCriticalSection;
critical_section::set_impl!(PowdrCriticalSection);

/// Powdr is strictly single threaded with no interrupts, so
/// critical section implementation is empty.
unsafe impl critical_section::Impl for PowdrCriticalSection {
    unsafe fn acquire() -> RawRestoreState {}
    unsafe fn release(token: RawRestoreState) {}
}
