mod psci_consts {
    pub(super) const PSCI_0_2_FN_BASE: u32 = 0x8400_0000;
    pub(super) const PSCI_0_2_64BIT: u32 = 0x4000_0000;
    pub(super) const PSCI_0_2_FN64_BASE: u32 = PSCI_0_2_FN_BASE + PSCI_0_2_64BIT;

    // 32-bit function IDs
    pub(super) const FN_VERSION: u32 = PSCI_0_2_FN_BASE;
    pub(super) const FN_CPU_SUSPEND: u32 = PSCI_0_2_FN_BASE + 1;
    pub(super) const FN_CPU_OFF: u32 = PSCI_0_2_FN_BASE + 2;
    pub(super) const FN_CPU_ON: u32 = PSCI_0_2_FN_BASE + 3;
    pub(super) const FN_AFFINITY_INFO: u32 = PSCI_0_2_FN_BASE + 4;
    pub(super) const FN_MIGRATE: u32 = PSCI_0_2_FN_BASE + 5;
    pub(super) const FN_MIGRATE_INFO_TYPE: u32 = PSCI_0_2_FN_BASE + 6;
    pub(super) const FN_MIGRATE_INFO_UP_CPU: u32 = PSCI_0_2_FN_BASE + 7;
    pub(super) const FN_SYSTEM_OFF: u32 = PSCI_0_2_FN_BASE + 8;
    pub(super) const FN_SYSTEM_RESET: u32 = PSCI_0_2_FN_BASE + 9;
    pub(super) const FN_PSCI_FEATURES: u32 = PSCI_0_2_FN_BASE + 10;
    pub(super) const FN_SYSTEM_SUSPEND: u32 = PSCI_0_2_FN_BASE + 14;
    pub(super) const FN_SET_SUSPEND_MODE: u32 = PSCI_0_2_FN_BASE + 15;
    pub(super) const FN_SYSTEM_RESET2: u32 = PSCI_0_2_FN_BASE + 18;

    // 64-bit function IDs
    pub(super) const FN64_CPU_SUSPEND: u32 = PSCI_0_2_FN64_BASE + 1;
    pub(super) const FN64_CPU_ON: u32 = PSCI_0_2_FN64_BASE + 3;
    pub(super) const FN64_AFFINITY_INFO: u32 = PSCI_0_2_FN64_BASE + 4;
    pub(super) const FN64_MIGRATE: u32 = PSCI_0_2_FN64_BASE + 5;
    pub(super) const FN64_MIGRATE_INFO_UP_CPU: u32 = PSCI_0_2_FN64_BASE + 7;
    pub(super) const FN64_SYSTEM_SUSPEND: u32 = PSCI_0_2_FN64_BASE + 14;
    pub(super) const FN64_SYSTEM_RESET2: u32 = PSCI_0_2_FN64_BASE + 18;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PsciFunction {
    Version,
    CpuSuspend,
    CpuOff,
    CpuOn,
    AffinityInfo,
    Migrate,
    MigrateInfoType,
    MigrateInfoUpCpu,
    SystemOff,
    SystemReset,
    PsciFeatures,
    SystemSuspend,
    SetSuspendMode,
    SystemReset2,
    // 64-bit variants
    CpuSuspend64,
    CpuOn64,
    AffinityInfo64,
    Migrate64,
    MigrateInfoUpCpu64,
    SystemSuspend64,
    SystemReset264,
    // Catch-all
    Unknown(u32),
}

impl From<u32> for PsciFunction {
    fn from(code: u32) -> Self {
        match code {
            psci_consts::FN_VERSION => PsciFunction::Version,
            psci_consts::FN_CPU_SUSPEND => PsciFunction::CpuSuspend,
            psci_consts::FN_CPU_OFF => PsciFunction::CpuOff,
            psci_consts::FN_CPU_ON => PsciFunction::CpuOn,
            psci_consts::FN_AFFINITY_INFO => PsciFunction::AffinityInfo,
            psci_consts::FN_MIGRATE => PsciFunction::Migrate,
            psci_consts::FN_MIGRATE_INFO_TYPE => PsciFunction::MigrateInfoType,
            psci_consts::FN_MIGRATE_INFO_UP_CPU => PsciFunction::MigrateInfoUpCpu,
            psci_consts::FN_SYSTEM_OFF => PsciFunction::SystemOff,
            psci_consts::FN_SYSTEM_RESET => PsciFunction::SystemReset,
            psci_consts::FN_PSCI_FEATURES => PsciFunction::PsciFeatures,
            psci_consts::FN_SYSTEM_SUSPEND => PsciFunction::SystemSuspend,
            psci_consts::FN_SET_SUSPEND_MODE => PsciFunction::SetSuspendMode,
            psci_consts::FN_SYSTEM_RESET2 => PsciFunction::SystemReset2,

            psci_consts::FN64_CPU_SUSPEND => PsciFunction::CpuSuspend64,
            psci_consts::FN64_CPU_ON => PsciFunction::CpuOn64,
            psci_consts::FN64_AFFINITY_INFO => PsciFunction::AffinityInfo64,
            psci_consts::FN64_MIGRATE => PsciFunction::Migrate64,
            psci_consts::FN64_MIGRATE_INFO_UP_CPU => PsciFunction::MigrateInfoUpCpu64,
            psci_consts::FN64_SYSTEM_SUSPEND => PsciFunction::SystemSuspend64,
            psci_consts::FN64_SYSTEM_RESET2 => PsciFunction::SystemReset264,

            unknown_code => PsciFunction::Unknown(unknown_code),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PsciReturn {
    Success,
    NotSupported,
    InvalidParams,
    Denied,
    AlreadyOn,
    OnPending,
    InternalFailure,
    NotPresent,
    Disabled,
    InvalidAddress,
    Unknown(i32),
}

impl From<i32> for PsciReturn {
    fn from(code: i32) -> Self {
        match code {
            0 => PsciReturn::Success,
            -1 => PsciReturn::NotSupported,
            -2 => PsciReturn::InvalidParams,
            -3 => PsciReturn::Denied,
            -4 => PsciReturn::AlreadyOn,
            -5 => PsciReturn::OnPending,
            -6 => PsciReturn::InternalFailure,
            -7 => PsciReturn::NotPresent,
            -8 => PsciReturn::Disabled,
            -9 => PsciReturn::InvalidAddress,
            c => PsciReturn::Unknown(c),
        }
    }
}

impl From<PsciReturn> for i32 {
    fn from(code: PsciReturn) -> Self {
        match code {
            PsciReturn::Success => 0,
            PsciReturn::NotSupported => -1,
            PsciReturn::InvalidParams => -2,
            PsciReturn::Denied => -3,
            PsciReturn::AlreadyOn => -4,
            PsciReturn::OnPending => -5,
            PsciReturn::InternalFailure => -6,
            PsciReturn::NotPresent => -7,
            PsciReturn::Disabled => -8,
            PsciReturn::InvalidAddress => -9,
            PsciReturn::Unknown(c) => c,
        }
    }
}
