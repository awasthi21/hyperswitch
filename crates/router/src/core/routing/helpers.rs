//! Analysis for usage of all helper functions for use case of routing
//!
//! Functions that are used to perform the retrieval of merchant's
//! routing dict, configs, defaults
use api_models::routing as routing_types;
use common_utils::ext_traits::Encode;
use diesel_models::{
    business_profile::{BusinessProfile, BusinessProfileUpdate},
    configs,
};
use error_stack::ResultExt;
use rustc_hash::FxHashSet;
use storage_impl::redis::cache;

use crate::{
    core::errors::{self, RouterResult},
    db::StorageInterface,
    routes::SessionState,
    types::{domain, storage},
    utils::StringExt,
};

/// provides the complete merchant routing dictionary that is basically a list of all the routing
/// configs a merchant configured with an active_id field that specifies the current active routing
/// config
// pub async fn get_merchant_routing_dictionary(
//     db: &dyn StorageInterface,
//     merchant_id: &str,
// ) -> RouterResult<routing_types::RoutingDictionary> {
//     let key = get_routing_dictionary_key(merchant_id);
//     let maybe_dict = db.find_config_by_key(&key).await;

//     match maybe_dict {
//         Ok(config) => config
//             .config
//             .parse_struct("RoutingDictionary")
//             .change_context(errors::ApiErrorResponse::InternalServerError)
//             .attach_printable("Merchant routing dictionary has invalid structure"),

//         Err(e) if e.current_context().is_db_not_found() => {
//             let new_dictionary = routing_types::RoutingDictionary {
//                 merchant_id: merchant_id.to_owned(),
//                 active_id: None,
//                 records: Vec::new(),
//             };

//             let serialized = new_dictionary
//                 .encode_to_string_of_json()
//                 .change_context(errors::ApiErrorResponse::InternalServerError)
//                 .attach_printable("Error serializing newly created merchant dictionary")?;

//             let new_config = configs::ConfigNew {
//                 key,
//                 config: serialized,
//             };

//             db.insert_config(new_config)
//                 .await
//                 .change_context(errors::ApiErrorResponse::InternalServerError)
//                 .attach_printable("Error inserting new routing dictionary for merchant")?;

//             Ok(new_dictionary)
//         }

//         Err(e) => Err(e)
//             .change_context(errors::ApiErrorResponse::InternalServerError)
//             .attach_printable("Error fetching routing dictionary for merchant"),
//     }
// }

/// Provides us with all the configured configs of the Merchant in the ascending time configured
/// manner and chooses the first of them
pub async fn get_merchant_default_config(
    db: &dyn StorageInterface,
    // Cannot make this as merchant id domain type because, we are passing profile id also here
    merchant_id: &str,
    transaction_type: &storage::enums::TransactionType,
) -> RouterResult<Vec<routing_types::RoutableConnectorChoice>> {
    let key = get_default_config_key(merchant_id, transaction_type);
    let maybe_config = db.find_config_by_key(&key).await;

    match maybe_config {
        Ok(config) => config
            .config
            .parse_struct("Vec<RoutableConnectors>")
            .change_context(errors::ApiErrorResponse::InternalServerError)
            .attach_printable("Merchant default config has invalid structure"),

        Err(e) if e.current_context().is_db_not_found() => {
            let new_config_conns = Vec::<routing_types::RoutableConnectorChoice>::new();
            let serialized = new_config_conns
                .encode_to_string_of_json()
                .change_context(errors::ApiErrorResponse::InternalServerError)
                .attach_printable(
                    "Error while creating and serializing new merchant default config",
                )?;

            let new_config = configs::ConfigNew {
                key,
                config: serialized,
            };

            db.insert_config(new_config)
                .await
                .change_context(errors::ApiErrorResponse::InternalServerError)
                .attach_printable("Error inserting new default routing config into DB")?;

            Ok(new_config_conns)
        }

        Err(e) => Err(e)
            .change_context(errors::ApiErrorResponse::InternalServerError)
            .attach_printable("Error fetching default config for merchant"),
    }
}

/// Merchant's already created config can be updated and this change will be reflected
/// in DB as well for the particular updated config
pub async fn update_merchant_default_config(
    db: &dyn StorageInterface,
    merchant_id: &str,
    connectors: Vec<routing_types::RoutableConnectorChoice>,
    transaction_type: &storage::enums::TransactionType,
) -> RouterResult<()> {
    let key = get_default_config_key(merchant_id, transaction_type);
    let config_str = connectors
        .encode_to_string_of_json()
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Unable to serialize merchant default routing config during update")?;

    let config_update = configs::ConfigUpdate::Update {
        config: Some(config_str),
    };

    db.update_config_by_key(&key, config_update)
        .await
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Error updating the default routing config in DB")?;

    Ok(())
}

pub async fn update_merchant_routing_dictionary(
    db: &dyn StorageInterface,
    merchant_id: &str,
    dictionary: routing_types::RoutingDictionary,
) -> RouterResult<()> {
    let key = get_routing_dictionary_key(merchant_id);
    let dictionary_str = dictionary
        .encode_to_string_of_json()
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Unable to serialize routing dictionary during update")?;

    let config_update = configs::ConfigUpdate::Update {
        config: Some(dictionary_str),
    };

    db.update_config_by_key(&key, config_update)
        .await
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Error saving routing dictionary to DB")?;

    Ok(())
}

pub async fn update_routing_algorithm(
    db: &dyn StorageInterface,
    algorithm_id: String,
    algorithm: routing_types::RoutingAlgorithm,
) -> RouterResult<()> {
    let algorithm_str = algorithm
        .encode_to_string_of_json()
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Unable to serialize routing algorithm to string")?;

    let config_update = configs::ConfigUpdate::Update {
        config: Some(algorithm_str),
    };

    db.update_config_by_key(&algorithm_id, config_update)
        .await
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Error updating the routing algorithm in DB")?;

    Ok(())
}

/// This will help make one of all configured algorithms to be in active state for a particular
/// merchant
pub async fn update_merchant_active_algorithm_ref(
    state: &SessionState,
    key_store: &domain::MerchantKeyStore,
    config_key: cache::CacheKind<'_>,
    algorithm_id: routing_types::RoutingAlgorithmRef,
) -> RouterResult<()> {
    let ref_value = algorithm_id
        .encode_to_value()
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Failed converting routing algorithm ref to json value")?;

    let merchant_account_update = storage::MerchantAccountUpdate::Update {
        merchant_name: None,
        merchant_details: None,
        return_url: None,
        webhook_details: None,
        sub_merchants_enabled: None,
        parent_merchant_id: None,
        enable_payment_response_hash: None,
        payment_response_hash_key: None,
        redirect_to_merchant_with_http_post: None,
        publishable_key: None,
        locker_id: None,
        metadata: None,
        routing_algorithm: Some(ref_value),
        primary_business_details: None,
        intent_fulfillment_time: None,
        frm_routing_algorithm: None,
        payout_routing_algorithm: None,
        default_profile: None,
        payment_link_config: None,
        pm_collect_link_config: None,
    };
    let db = &*state.store;
    db.update_specific_fields_in_merchant(
        &state.into(),
        &key_store.merchant_id,
        merchant_account_update,
        key_store,
    )
    .await
    .change_context(errors::ApiErrorResponse::InternalServerError)
    .attach_printable("Failed to update routing algorithm ref in merchant account")?;

    cache::publish_into_redact_channel(db.get_cache_store().as_ref(), [config_key])
        .await
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Failed to invalidate the config cache")?;

    Ok(())
}

pub async fn update_business_profile_active_algorithm_ref(
    db: &dyn StorageInterface,
    current_business_profile: BusinessProfile,
    algorithm_id: routing_types::RoutingAlgorithmRef,
    transaction_type: &storage::enums::TransactionType,
) -> RouterResult<()> {
    let ref_val = algorithm_id
        .encode_to_value()
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Failed to convert routing ref to value")?;

    let merchant_id = current_business_profile.merchant_id.clone();

    let profile_id = current_business_profile.profile_id.clone();

    let routing_cache_key = cache::CacheKind::Routing(
        format!(
            "routing_config_{}_{profile_id}",
            merchant_id.get_string_repr()
        )
        .into(),
    );

    let (routing_algorithm, payout_routing_algorithm) = match transaction_type {
        storage::enums::TransactionType::Payment => (Some(ref_val), None),
        #[cfg(feature = "payouts")]
        storage::enums::TransactionType::Payout => (None, Some(ref_val)),
    };

    let business_profile_update = BusinessProfileUpdate::Update {
        profile_name: None,
        return_url: None,
        enable_payment_response_hash: None,
        payment_response_hash_key: None,
        redirect_to_merchant_with_http_post: None,
        webhook_details: None,
        metadata: None,
        routing_algorithm,
        intent_fulfillment_time: None,
        frm_routing_algorithm: None,
        payout_routing_algorithm,
        applepay_verified_domains: None,
        modified_at: None,
        is_recon_enabled: None,
        payment_link_config: None,
        session_expiry: None,
        authentication_connector_details: None,
        payout_link_config: None,
        extended_card_info_config: None,
        use_billing_as_payment_method_billing: None,
        collect_shipping_details_from_wallet_connector: None,
        collect_billing_details_from_wallet_connector: None,
        is_connector_agnostic_mit_enabled: None,
        outgoing_webhook_custom_http_headers: None,
    };

    db.update_business_profile_by_profile_id(current_business_profile, business_profile_update)
        .await
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Failed to update routing algorithm ref in business profile")?;

    cache::publish_into_redact_channel(db.get_cache_store().as_ref(), [routing_cache_key])
        .await
        .change_context(errors::ApiErrorResponse::InternalServerError)
        .attach_printable("Failed to invalidate routing cache")?;
    Ok(())
}

pub async fn validate_connectors_in_routing_config(
    state: &SessionState,
    key_store: &domain::MerchantKeyStore,
    merchant_id: &common_utils::id_type::MerchantId,
    profile_id: &str,
    routing_algorithm: &routing_types::RoutingAlgorithm,
) -> RouterResult<()> {
    let all_mcas = &*state
        .store
        .find_merchant_connector_account_by_merchant_id_and_disabled_list(
            &state.into(),
            merchant_id,
            true,
            key_store,
        )
        .await
        .change_context(errors::ApiErrorResponse::MerchantConnectorAccountNotFound {
            id: merchant_id.get_string_repr().to_owned(),
        })?;

    let name_mca_id_set = all_mcas
        .iter()
        .filter(|mca| mca.profile_id.as_deref() == Some(profile_id))
        .map(|mca| (&mca.connector_name, &mca.merchant_connector_id))
        .collect::<FxHashSet<_>>();

    let name_set = all_mcas
        .iter()
        .filter(|mca| mca.profile_id.as_deref() == Some(profile_id))
        .map(|mca| &mca.connector_name)
        .collect::<FxHashSet<_>>();

    let check_connector_choice = |choice: &routing_types::RoutableConnectorChoice| {
        if let Some(ref mca_id) = choice.merchant_connector_id {
            error_stack::ensure!(
                name_mca_id_set.contains(&(&choice.connector.to_string(), mca_id)),
                errors::ApiErrorResponse::InvalidRequestData {
                    message: format!(
                        "connector with name '{}' and merchant connector account id '{}' not found for the given profile",
                        choice.connector,
                        mca_id,
                    )
                }
            );
        } else {
            error_stack::ensure!(
                name_set.contains(&choice.connector.to_string()),
                errors::ApiErrorResponse::InvalidRequestData {
                    message: format!(
                        "connector with name '{}' not found for the given profile",
                        choice.connector,
                    )
                }
            );
        }

        Ok(())
    };

    match routing_algorithm {
        routing_types::RoutingAlgorithm::Single(choice) => {
            check_connector_choice(choice)?;
        }

        routing_types::RoutingAlgorithm::Priority(list) => {
            for choice in list {
                check_connector_choice(choice)?;
            }
        }

        routing_types::RoutingAlgorithm::VolumeSplit(splits) => {
            for split in splits {
                check_connector_choice(&split.connector)?;
            }
        }

        routing_types::RoutingAlgorithm::Advanced(program) => {
            let check_connector_selection =
                |selection: &routing_types::ConnectorSelection| -> RouterResult<()> {
                    match selection {
                        routing_types::ConnectorSelection::VolumeSplit(splits) => {
                            for split in splits {
                                check_connector_choice(&split.connector)?;
                            }
                        }

                        routing_types::ConnectorSelection::Priority(list) => {
                            for choice in list {
                                check_connector_choice(choice)?;
                            }
                        }
                    }

                    Ok(())
                };

            check_connector_selection(&program.default_selection)?;

            for rule in &program.rules {
                check_connector_selection(&rule.connector_selection)?;
            }
        }
    }

    Ok(())
}

/// Provides the identifier for the specific merchant's routing_dictionary_key
#[inline(always)]
pub fn get_routing_dictionary_key(merchant_id: &str) -> String {
    format!("routing_dict_{merchant_id}")
}

/// Provides the identifier for the specific merchant's default_config
#[inline(always)]
pub fn get_default_config_key(
    merchant_id: &str,
    transaction_type: &storage::enums::TransactionType,
) -> String {
    match transaction_type {
        storage::enums::TransactionType::Payment => format!("routing_default_{merchant_id}"),
        #[cfg(feature = "payouts")]
        storage::enums::TransactionType::Payout => format!("routing_default_po_{merchant_id}"),
    }
}
