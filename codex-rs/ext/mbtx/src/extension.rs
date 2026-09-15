use std::sync::Arc;

use codex_config::mbtx::MbtxConfig;
use codex_extension_api::ConfigContributor;
use codex_extension_api::ExtensionData;
use codex_extension_api::ExtensionFuture;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_extension_api::ThreadLifecycleContributor;
use codex_extension_api::ThreadStartInput;
use codex_extension_api::ToolContributor;
use codex_tools::ToolCall;
use codex_tools::ToolExecutor;

use crate::tool::MbtxTool;

struct MbtxExtension<C> {
    settings: fn(&C) -> MbtxConfig,
}

impl<C: Send + Sync> ThreadLifecycleContributor<C> for MbtxExtension<C> {
    fn on_thread_start<'a>(&'a self, input: ThreadStartInput<'a, C>) -> ExtensionFuture<'a, ()> {
        Box::pin(async move {
            input.thread_store.insert((self.settings)(input.config));
        })
    }
}

impl<C: Send + Sync> ConfigContributor<C> for MbtxExtension<C> {
    fn on_config_changed(&self, _: &ExtensionData, store: &ExtensionData, _: &C, config: &C) {
        store.insert((self.settings)(config));
    }
}

impl<C: Send + Sync> ToolContributor for MbtxExtension<C> {
    fn tools(
        &self,
        _: &ExtensionData,
        store: &ExtensionData,
    ) -> Vec<Arc<dyn for<'a> ToolExecutor<ToolCall<'a>>>> {
        store
            .get::<MbtxConfig>()
            .filter(|config| config.enabled)
            .map(|config| {
                vec![Arc::new(MbtxTool {
                    config: (*config).clone(),
                })
                    as Arc<dyn for<'a> ToolExecutor<ToolCall<'a>>>]
            })
            .unwrap_or_default()
    }
}

/// Install the optional tool without depending on the Codex host implementation.
pub fn install<C: Send + Sync + 'static>(
    registry: &mut ExtensionRegistryBuilder<C>,
    settings: fn(&C) -> MbtxConfig,
) {
    let extension = Arc::new(MbtxExtension { settings });
    registry.thread_lifecycle_contributor(extension.clone());
    registry.config_contributor(extension.clone());
    registry.tool_contributor(extension);
}
