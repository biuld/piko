use piko_protocol::messages::UsageCostBasis;
use std::sync::Arc;

use crate::billing::{BillableUsage, BillingContext, BillingRegistry, PricingPolicy};
use crate::modeling::BillingPlan;

use super::*;

struct FlatPolicy;

impl PricingPolicy for FlatPolicy {
    fn id(&self) -> &'static str {
        "test_flat"
    }

    fn validate(&self, plan: &BillingPlan) -> Result<(), String> {
        plan.configuration
            .get("price")
            .and_then(serde_json::Value::as_f64)
            .map(|_| ())
            .ok_or_else(|| "flat policy requires price".into())
    }

    fn estimate(
        &self,
        _: &BillingContext<'_>,
        plan: &BillingPlan,
        _: &BillableUsage,
    ) -> Result<piko_protocol::messages::UsageCost, String> {
        let price = plan.configuration["price"].as_f64().unwrap();
        Ok(piko_protocol::messages::UsageCost {
            entries: vec![piko_protocol::messages::UsageCostEntry {
                currency: plan.currency.clone(),
                basis: plan.basis,
                components: [("request".into(), price)].into(),
                total: price,
            }],
        })
    }
}

#[test]
fn custom_registry_validates_custom_catalog_policy() {
    let manifest = r#"
[provider]
id = "custom"
[api_surfaces.platform]
base_url = "https://example.com/v1"
auth_methods = ["api_key"]
[default_targets.platform]
protocol = "chat_completions"
[models.metered]
name = "Metered"
reasoning = false
input = ["text"]
context_window = 1000
max_tokens = 100
[[models.metered.pricing]]
surface = "platform"
basis = "list_price"
currency = "EUR"
policy = "test_flat"
[models.metered.pricing.configuration]
price = 0.25
"#;
    let mut billing = BillingRegistry::standard();
    billing
        .register_pricing_policy(Arc::new(FlatPolicy))
        .unwrap();
    let provider = load_provider_from_toml_with_billing(manifest, &billing).unwrap();
    let plan = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "metered")
        .unwrap()
        .billing
        .unwrap();
    assert_eq!(plan.pricing_policy, "test_flat");
    assert_eq!(plan.configuration["price"], 0.25);
}

#[test]
fn deepseek_catalog_selects_protocol_and_cny_pricing_per_model() {
    let provider = load_fixture_provider("deepseek").unwrap();
    let summaries = provider.list_models();
    let flash_summary = summaries
        .iter()
        .find(|model| model.id == "deepseek-v4-flash")
        .unwrap();
    assert!(
        flash_summary
            .tool_execution_loci
            .contains(&piko_protocol::model::ToolExecutionLocus::Upstream)
    );
    for model in ["deepseek-chat", "deepseek-reasoner"] {
        let summary = summaries
            .iter()
            .find(|summary| summary.id == model)
            .unwrap();
        assert_eq!(
            summary.tool_execution_loci,
            [piko_protocol::model::ToolExecutionLocus::Caller]
        );
    }
    let flash = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-flash")
        .unwrap();
    assert_eq!(
        flash.protocol,
        ProtocolProfile::Responses {
            continuation: ResponsesContinuationPolicy::StatelessReplay,
            variant: ResponsesVariant::Standard,
        }
    );
    let search_kind = crate::capabilities::UpstreamToolKind::new("search").unwrap();
    let search = &flash.upstream_tools[&search_kind];
    assert_eq!(search.name, "web_search");
    assert_eq!(
        search.wire_definition,
        serde_json::json!({"type":"web_search"})
    );
    assert_eq!(search.wire_choice, serde_json::json!({"type":"web_search"}));
    assert_eq!(search.activity_types, ["web_search_call"]);
    assert_eq!(
        flash
            .reasoning_effort_map
            .get(&ThinkingLevel::XHigh)
            .map(String::as_str),
        Some("max")
    );
    assert_eq!(
        provider
            .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-pro")
            .unwrap()
            .protocol,
        ProtocolProfile::Responses {
            continuation: ResponsesContinuationPolicy::StatelessReplay,
            variant: ResponsesVariant::Standard,
        }
    );
    let flash_pricing = flash.billing.unwrap();
    assert_eq!(flash_pricing.currency, "CNY");
    assert_eq!(flash_pricing.basis, UsageCostBasis::ListPrice);
    assert_eq!(flash_pricing.usage_adapter, "semantic_tokens");
    assert_eq!(flash_pricing.pricing_policy, "time_of_day");
    let flash_schedule: crate::billing::TimeOfDayPricing =
        serde_json::from_value(flash_pricing.configuration).unwrap();
    assert_eq!(flash_schedule.utc_offset, "+08:00");
    assert_eq!(flash_schedule.default.input_per_million, 1.5);
    assert_eq!(flash_schedule.default.cached_input_per_million, 0.05);
    assert_eq!(flash_schedule.default.output_per_million, 4.5);
    assert_eq!(flash_schedule.windows.len(), 2);
    let flash_peak = &flash_schedule.windows[0].rates;
    assert_eq!(flash_schedule.windows[0].start, "09:00");
    assert_eq!(flash_schedule.windows[0].end, "12:00");
    assert_eq!(flash_peak.input_per_million, 3.0);
    assert_eq!(flash_peak.cached_input_per_million, 0.10);
    assert_eq!(flash_peak.output_per_million, 9.0);
    let pro_pricing = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-pro")
        .unwrap()
        .billing
        .unwrap();
    assert_eq!(pro_pricing.currency, "CNY");
    assert_eq!(pro_pricing.pricing_policy, "time_of_day");
    let pro_schedule: crate::billing::TimeOfDayPricing =
        serde_json::from_value(pro_pricing.configuration).unwrap();
    assert_eq!(pro_schedule.utc_offset, "+08:00");
    assert_eq!(pro_schedule.default.input_per_million, 4.5);
    assert_eq!(pro_schedule.default.cached_input_per_million, 0.15);
    assert_eq!(pro_schedule.default.output_per_million, 13.5);
    assert_eq!(pro_schedule.windows.len(), 2);
    let pro_peak = &pro_schedule.windows[1].rates;
    assert_eq!(pro_schedule.windows[1].start, "14:00");
    assert_eq!(pro_schedule.windows[1].end, "18:00");
    assert_eq!(pro_peak.input_per_million, 9.0);
    assert_eq!(pro_peak.cached_input_per_million, 0.30);
    assert_eq!(pro_peak.output_per_million, 27.0);

    let vision_summary = summaries
        .iter()
        .find(|model| model.id == "deepseek-v4-flash-vision-exp")
        .unwrap();
    assert_eq!(vision_summary.context_window, 1_000_000);
    assert_eq!(vision_summary.max_tokens, 384_000);
    assert!(
        vision_summary
            .input
            .contains(&piko_protocol::model::InputModality::Image)
    );
    assert!(
        vision_summary
            .tool_execution_loci
            .contains(&piko_protocol::model::ToolExecutionLocus::Upstream)
    );
    let vision = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-flash-vision-exp")
        .unwrap();
    assert_eq!(
        vision.protocol,
        ProtocolProfile::Responses {
            continuation: ResponsesContinuationPolicy::StatelessReplay,
            variant: ResponsesVariant::Standard,
        }
    );
    assert!(vision.upstream_tools.contains_key(&search_kind));
    let vision_billing = vision.billing.as_ref().unwrap();
    let vision_schedule: crate::billing::TimeOfDayPricing =
        serde_json::from_value(vision_billing.configuration.clone()).unwrap();
    assert_eq!(vision_schedule.default.input_per_million, 1.5);
    assert_eq!(vision_schedule.default.cached_input_per_million, 0.05);
    assert_eq!(vision_schedule.default.output_per_million, 4.5);
    let image_usage = piko_protocol::messages::Usage {
        input: 1_000_000,
        total_tokens: 1_000_000,
        ..Default::default()
    };
    let occurred_at = chrono::DateTime::parse_from_rfc3339("2026-08-20T05:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let image_cost = BillingRegistry::standard()
        .estimate(
            &BillingContext {
                provider: "deepseek",
                model: "deepseek-v4-flash-vision-exp",
                api_surface: "platform",
                occurred_at,
            },
            vision_billing,
            &image_usage,
        )
        .unwrap();
    assert_eq!(image_cost.entries[0].total, 1.5);

    for model in ["deepseek-chat", "deepseek-reasoner"] {
        assert!(
            provider
                .target_for_model(ProviderAuthMethod::ApiKey, model)
                .unwrap()
                .upstream_tools
                .is_empty()
        );
    }
}

#[test]
fn openai_catalog_includes_gpt_5_6_capabilities_and_surface_pricing() {
    let provider = load_fixture_provider("openai").unwrap();
    let models = provider.list_models();
    for id in ["gpt-5.6", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
        let model = models.iter().find(|model| model.id == id).unwrap();
        assert_eq!(model.context_window, 1_050_000);
        assert_eq!(model.max_tokens, 128_000);
        assert_eq!(
            model.reasoning_efforts,
            vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::Medium,
                ThinkingLevel::High,
                ThinkingLevel::XHigh,
                ThinkingLevel::Max,
            ]
        );
        let target = provider
            .target_for_model(ProviderAuthMethod::ApiKey, id)
            .unwrap();
        assert_eq!(
            target
                .reasoning_effort_map
                .get(&ThinkingLevel::Max)
                .map(String::as_str),
            Some("max")
        );
        let platform_pricing = target.billing.unwrap();
        assert_eq!(platform_pricing.currency, "USD");
        assert_eq!(platform_pricing.basis, UsageCostBasis::ListPrice);
        let platform_schedule: crate::billing::StandardTokenPricing =
            serde_json::from_value(platform_pricing.configuration).unwrap();
        assert_eq!(platform_schedule.tiers[0].input_tokens_above, 272_000);
        let subscription_pricing = provider
            .target_for_model(ProviderAuthMethod::OAuth, id)
            .unwrap()
            .billing
            .unwrap();
        assert_eq!(subscription_pricing.basis, UsageCostBasis::ApiEquivalent);
        assert_eq!(subscription_pricing.currency, "USD");
    }
}
