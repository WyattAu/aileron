use crate::extensions::manifest::ExtensionManifest;
use crate::extensions::runtime::RuntimeApi;
use crate::extensions::scripting::ScriptingApi;
use crate::extensions::storage::StorageApi;
use crate::extensions::tabs::TabsApi;
use crate::extensions::types::ExtensionId;
use crate::extensions::web_request::WebRequestApi;

/// Extension API surface — the single entry point for an extension.
/// Each extension gets its own `ExtensionApi` instance with its
/// manifest's permissions enforced.
pub trait ExtensionApi: Send + Sync {
    fn id(&self) -> &ExtensionId;

    fn manifest(&self) -> &ExtensionManifest;

    fn tabs(&self) -> &dyn TabsApi;

    fn storage(&self) -> &dyn StorageApi;

    fn runtime(&self) -> &dyn RuntimeApi;

    fn web_request(&self) -> &dyn WebRequestApi;

    fn scripting(&self) -> &dyn ScriptingApi;
}
