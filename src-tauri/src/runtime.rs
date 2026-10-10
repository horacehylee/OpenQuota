use std::{path::Path, sync::Arc};

use thiserror::Error;

use crate::{
    pricing::PricingStore,
    providers::{
        antigravity::AntigravityProvider, claude, codex::CodexProvider, copilot::CopilotProvider,
        cursor::CursorProvider, devin::DevinProvider, grok::GrokProvider, kimi::KimiProvider,
        minimax::MiniMaxProvider, opencode::OpenCodeProvider, openrouter::OpenRouterProvider,
        zai::ZaiProvider, ProviderRegistry, ProviderRegistryError, UsageProvider,
    },
    service::ProviderService,
    settings::{CredentialDetectionPlan, SettingsService},
    storage::Storage,
};

#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Claude(#[from] claude::ClaudeError),
    #[error(transparent)]
    Pricing(#[from] crate::pricing::PricingStoreError),
    #[error(transparent)]
    Registry(#[from] ProviderRegistryError),
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageError),
    #[error("{0}")]
    Message(String),
}

impl CoreError {
    /// Provider constructors each return their own error type; the safe display message is
    /// all the CLI and GUI need.
    fn message(error: impl std::fmt::Display) -> Self {
        Self::Message(error.to_string())
    }
}

pub struct Core {
    pub storage: Arc<Storage>,
    pub registry: Arc<ProviderRegistry>,
    pub settings: Arc<SettingsService>,
    pub service: Arc<ProviderService>,
    pub credential_detection_plan: CredentialDetectionPlan,
}

/// Builds the shared provider stack (storage, pricing, registry, settings, service) from an
/// OpenQuota data directory. The GUI setup hook and the CLI both start here, so they see the
/// same providers, settings, and cached snapshots.
pub fn build_core(data_dir: &Path) -> Result<Core, CoreError> {
    let storage = Arc::new(Storage::open(&data_dir.join("openquota.db"))?);
    crate::provider_environment::initialize(storage.load_provider_environment()?);
    crate::app_debug!("cache", "application database opened");
    let pricing = Arc::new(PricingStore::new(data_dir.join("pricing"))?);
    let mut providers = claude::runtimes(storage.clone(), pricing.clone())?;
    providers.extend(vec![
        Arc::new(CodexProvider::new(storage.clone(), pricing.clone()).map_err(CoreError::message)?)
            as Arc<dyn UsageProvider>,
        Arc::new(CursorProvider::new(pricing.clone()).map_err(CoreError::message)?)
            as Arc<dyn UsageProvider>,
        Arc::new(
            AntigravityProvider::new(data_dir.join("antigravity").join("auth.json"))
                .map_err(CoreError::message)?,
        ) as Arc<dyn UsageProvider>,
        Arc::new(CopilotProvider::new().map_err(CoreError::message)?) as Arc<dyn UsageProvider>,
        Arc::new(DevinProvider::new().map_err(CoreError::message)?) as Arc<dyn UsageProvider>,
        Arc::new(GrokProvider::new(storage.clone(), pricing.clone()).map_err(CoreError::message)?)
            as Arc<dyn UsageProvider>,
        Arc::new(OpenCodeProvider::new(pricing.clone())) as Arc<dyn UsageProvider>,
        Arc::new(OpenRouterProvider::new().map_err(CoreError::message)?) as Arc<dyn UsageProvider>,
        Arc::new(ZaiProvider::new(pricing.clone()).map_err(CoreError::message)?)
            as Arc<dyn UsageProvider>,
        Arc::new(KimiProvider::new().map_err(CoreError::message)?) as Arc<dyn UsageProvider>,
        Arc::new(MiniMaxProvider::new().map_err(CoreError::message)?) as Arc<dyn UsageProvider>,
    ]);
    let registry = Arc::new(ProviderRegistry::new(providers)?);
    let (settings, credential_detection_plan) =
        SettingsService::new_deferred(storage.clone(), registry.clone())?;
    let settings = Arc::new(settings);
    let service = Arc::new(ProviderService::new_with_settings(
        registry.clone(),
        storage.clone(),
        settings.clone(),
    ));
    Ok(Core {
        storage,
        registry,
        settings,
        service,
        credential_detection_plan,
    })
}
