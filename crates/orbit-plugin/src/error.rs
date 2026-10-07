use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("E_COMMAND_NOT_FOUND: {name}")]
    CommandNotFound { name: String },

    #[error("E_CAPABILITY_DENIED: {name}")]
    CapabilityDenied { name: String },

    #[error("E_CAPABILITY_REVOKED: {name}")]
    CapabilityRevoked { name: String },

    #[error("E_CAPABILITY_INVALID_SCOPE: {name} scope {scope} not in allowed")]
    CapabilityInvalidScope { name: String, scope: String },

    #[error("E_HOOK_NOT_SUBSCRIBABLE: {topic}")]
    HookNotSubscribable { topic: String },

    #[error("E_COMMAND_INVALID_ARGS: {name} — {detail}")]
    CommandInvalidArgs { name: String, detail: String },

    #[error("E_COMMAND_RUNTIME_FAILED: {name} — {detail}")]
    CommandRuntimeFailed { name: String, detail: String },

    #[error("E_SURFACE_CONFIG_INVALID: {kind} — {detail}")]
    SurfaceConfigInvalid { kind: String, detail: String },

    #[error("E_BUS_PAYLOAD_TOO_LARGE: {size} bytes > 64KB")]
    BusPayloadTooLarge { size: usize },

    #[error("E_BUS_TOPIC_INVALID: {topic}")]
    BusTopicInvalid { topic: String },

    #[error("E_NETWORK_HOST_DENIED: {host}")]
    NetworkHostDenied { host: String },

    #[error("E_STORE_PATH_OUT_OF_SCOPE: {path}")]
    StorePathOutOfScope { path: String },

    #[error("E_STORE_OTHER_NOT_WHITELISTED: {namespace}")]
    StoreOtherNotWhitelisted { namespace: String },

    #[error("E_ANTI_SPAM_RATE_LIMIT: {operation}")]
    AntiSpamRateLimit { operation: String },

    #[error("E_PRIORITY_CONFLICT: {kind} priority conflict")]
    PriorityConflict { kind: String },

    #[error("plugin panicked: {0}")]
    Panicked(String),

    #[error("WASM trap: {0}")]
    WasmTrap(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("E_MANIFEST_NOT_FOUND: {path}")]
    ManifestNotFound { path: String },

    #[error("E_MANIFEST_INVALID_ENCODING: {path} is not UTF-8")]
    ManifestInvalidEncoding { path: String },

    #[error("E_MANIFEST_INVALID_JSON: {0}")]
    ManifestInvalidJson(String),

    #[error("E_MANIFEST_MISSING_FIELD: {0}")]
    ManifestMissingField(String),

    #[error("E_MANIFEST_UNKNOWN_FIELD: {0}")]
    ManifestUnknownField(String),

    #[error("E_MANIFEST_INVALID_ID: {0}")]
    ManifestInvalidId(String),

    #[error("E_MANIFEST_INVALID_VERSION: {0}")]
    ManifestInvalidVersion(String),

    #[error("E_MANIFEST_ENTRY_NOT_FOUND: {0}")]
    ManifestEntryNotFound(String),

    #[error("E_MANIFEST_RUNTIME_MISMATCH: runtime={runtime}, entry={entry}")]
    ManifestRuntimeMismatch { runtime: String, entry: String },

    #[error("E_MANIFEST_ORBIT_VERSION_INCOMPATIBLE: requires {required}, have {actual}")]
    OrbitVersionIncompatible { required: String, actual: String },

    #[error("E_MANIFEST_DUPLICATE_COMMAND_NAME: {0}")]
    DuplicateCommandName(String),

    #[error("E_DECL_UNKNOWN_KIND: {0}")]
    DeclUnknownKind(String),

    #[error("E_DECL_UNKNOWN_TOPIC: {0}")]
    DeclUnknownTopic(String),

    #[error("E_DECL_UNKNOWN_COMMAND: {0}")]
    DeclUnknownCommand(String),

    #[error("E_CAPABILITY_MISSING: {0}")]
    CapabilityMissing(String),

    #[error("E_DEPENDENCY_MISSING: {0}")]
    DependencyMissing(String),

    #[error("E_DEPENDENCY_VERSION_MISMATCH: {name} requires {required}, have {actual}")]
    DependencyVersionMismatch { name: String, required: String, actual: String },

    #[error("E_CONFIG_SCHEMA_INVALID: {0}")]
    ConfigSchemaInvalid(String),

    #[error("E_CONFIG_DEFAULT_INVALID: {0}")]
    ConfigDefaultInvalid(String),

    #[error("E_NETWORK_HOST_INVALID: {0}")]
    NetworkHostInvalid(String),

    #[error("E_NATIVE_NOT_SIGNED: {0}")]
    NativeNotSigned(String),

    #[error("E_NATIVE_HASH_MISMATCH: {0}")]
    NativeHashMismatch(String),

    #[error("E_NATIVE_CAPABILITY_MISSING: native plugin requires runtime.native capability")]
    NativeCapabilityMissing,

    #[error("E_NATIVE_ABI_MISMATCH: plugin ABI v{plugin}, host ABI v{host}")]
    NativeAbiMismatch { plugin: u32, host: u32 },

    #[error("native library load failed: {0}")]
    NativeLoad(String),

    #[error("WASM module compile failed: {0}")]
    WasmCompile(String),
}

#[derive(Debug, Error)]
pub enum UpgradeError {
    #[error("soft migration failed: {detail}")]
    SoftFailed { detail: String },

    #[error("hard migration failed: {detail}")]
    HardFailed { detail: String },
}
