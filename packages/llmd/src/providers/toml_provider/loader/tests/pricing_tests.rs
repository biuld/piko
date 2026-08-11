use piko_protocol::messages::UsageCostBasis;

use super::*;

#[test]
fn deepseek_catalog_selects_protocol_and_cny_pricing_per_model() {
    let provider = load_builtin_provider("deepseek").unwrap();
    let flash = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-flash")
        .unwrap();
    assert_eq!(
        flash.protocol,
        ProtocolProfile::Responses {
            continuation: ResponsesContinuationPolicy::StatelessReplay
        }
    );
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
        ProtocolProfile::ChatCompletions
    );
    let flash_pricing = flash.pricing.unwrap();
    assert_eq!(flash_pricing.currency, "CNY");
    assert_eq!(flash_pricing.basis, UsageCostBasis::ListPrice);
    assert_eq!(flash_pricing.input_per_million, 1.0);
    assert_eq!(flash_pricing.cached_input_per_million, 0.02);
    assert_eq!(flash_pricing.output_per_million, 2.0);
    let pro_pricing = provider
        .target_for_model(ProviderAuthMethod::ApiKey, "deepseek-v4-pro")
        .unwrap()
        .pricing
        .unwrap();
    assert_eq!(pro_pricing.currency, "CNY");
    assert_eq!(pro_pricing.input_per_million, 3.0);
    assert_eq!(pro_pricing.cached_input_per_million, 0.025);
    assert_eq!(pro_pricing.output_per_million, 6.0);
}

#[test]
fn openai_catalog_includes_gpt_5_6_capabilities_and_surface_pricing() {
    let provider = load_builtin_provider("openai").unwrap();
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
        let platform_pricing = target.pricing.unwrap();
        assert_eq!(platform_pricing.currency, "USD");
        assert_eq!(platform_pricing.basis, UsageCostBasis::ListPrice);
        assert_eq!(platform_pricing.tiers[0].input_tokens_above, 272_000);
        let subscription_pricing = provider
            .target_for_model(ProviderAuthMethod::OAuth, id)
            .unwrap()
            .pricing
            .unwrap();
        assert_eq!(subscription_pricing.basis, UsageCostBasis::ApiEquivalent);
        assert_eq!(subscription_pricing.currency, "USD");
    }
}
