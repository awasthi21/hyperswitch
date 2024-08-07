pub mod transformers;

use common_utils::{
    errors::ReportSwitchExt,
    ext_traits::ByteSliceExt,
    request::RequestContent,
    types::{AmountConvertor, MinorUnit, MinorUnitForConnector},
};
use error_stack::ResultExt;
use masking::PeekInterface;
use transformers as bitpay;

use self::bitpay::BitpayWebhookDetails;
use super::utils as connector_utils;
use crate::{
    configs::settings,
    consts,
    core::errors::{self, CustomResult},
    events::connector_api_logs::ConnectorEvent,
    headers,
    services::{
        self,
        request::{self, Mask},
        ConnectorIntegration, ConnectorValidation,
    },
    types::{
        self,
        api::{self, ConnectorCommon, ConnectorCommonExt},
        ErrorResponse, Response,
    },
    utils::BytesExt,
};

#[derive(Clone)]
pub struct Bitpay {
    amount_converter: &'static (dyn AmountConvertor<Output = MinorUnit> + Sync),
}

impl Bitpay {
    pub fn new() -> &'static Self {
        &Self {
            amount_converter: &MinorUnitForConnector,
        }
    }
}

impl api::Payment for Bitpay {}
impl api::PaymentToken for Bitpay {}
impl api::PaymentSession for Bitpay {}
impl api::ConnectorAccessToken for Bitpay {}
impl api::MandateSetup for Bitpay {}
impl api::PaymentAuthorize for Bitpay {}
impl api::PaymentSync for Bitpay {}
impl api::PaymentCapture for Bitpay {}
impl api::PaymentVoid for Bitpay {}
impl api::Refund for Bitpay {}
impl api::RefundExecute for Bitpay {}
impl api::RefundSync for Bitpay {}

impl
    ConnectorIntegration<
        api::PaymentMethodToken,
        types::PaymentMethodTokenizationData,
        types::PaymentsResponseData,
    > for Bitpay
{
    // Not Implemented (R)
}

impl<Flow, Request, Response> ConnectorCommonExt<Flow, Request, Response> for Bitpay
where
    Self: ConnectorIntegration<Flow, Request, Response>,
{
    fn build_headers(
        &self,
        _req: &types::RouterData<Flow, Request, Response>,
        _connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, request::Maskable<String>)>, errors::ConnectorError> {
        let header = vec![
            (
                headers::CONTENT_TYPE.to_string(),
                types::PaymentsAuthorizeType::get_content_type(self)
                    .to_string()
                    .into(),
            ),
            (
                headers::X_ACCEPT_VERSION.to_string(),
                "2.0.0".to_string().into(),
            ),
        ];
        Ok(header)
    }
}

impl ConnectorCommon for Bitpay {
    fn id(&self) -> &'static str {
        "bitpay"
    }

    fn get_currency_unit(&self) -> api::CurrencyUnit {
        api::CurrencyUnit::Minor
    }

    fn common_get_content_type(&self) -> &'static str {
        "application/json"
    }

    fn base_url<'a>(&self, connectors: &'a settings::Connectors) -> &'a str {
        connectors.bitpay.base_url.as_ref()
    }

    fn get_auth_header(
        &self,
        auth_type: &types::ConnectorAuthType,
    ) -> CustomResult<Vec<(String, request::Maskable<String>)>, errors::ConnectorError> {
        let auth = bitpay::BitpayAuthType::try_from(auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        Ok(vec![(
            headers::AUTHORIZATION.to_string(),
            auth.api_key.into_masked(),
        )])
    }

    fn build_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        let response: bitpay::BitpayErrorResponse =
            res.response.parse_struct("BitpayErrorResponse").switch()?;

        event_builder.map(|i| i.set_error_response_body(&response));
        router_env::logger::info!(connector_response=?response);

        Ok(ErrorResponse {
            status_code: res.status_code,
            code: response
                .code
                .unwrap_or_else(|| consts::NO_ERROR_CODE.to_string()),
            message: response.error,
            reason: response.message,
            attempt_status: None,
            connector_transaction_id: None,
        })
    }
}

impl ConnectorValidation for Bitpay {}

impl ConnectorIntegration<api::Session, types::PaymentsSessionData, types::PaymentsResponseData>
    for Bitpay
{
    //TODO: implement sessions flow
}

impl ConnectorIntegration<api::AccessTokenAuth, types::AccessTokenRequestData, types::AccessToken>
    for Bitpay
{
}

impl
    ConnectorIntegration<
        api::SetupMandate,
        types::SetupMandateRequestData,
        types::PaymentsResponseData,
    > for Bitpay
{
    fn build_request(
        &self,
        _req: &types::RouterData<
            api::SetupMandate,
            types::SetupMandateRequestData,
            types::PaymentsResponseData,
        >,
        _connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Setup Mandate flow for Bitpay".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<api::Authorize, types::PaymentsAuthorizeData, types::PaymentsResponseData>
    for Bitpay
{
    fn get_headers(
        &self,
        req: &types::PaymentsAuthorizeRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, request::Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        _req: &types::PaymentsAuthorizeRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        Ok(format!("{}/invoices", self.base_url(connectors)))
    }

    fn get_request_body(
        &self,
        req: &types::PaymentsAuthorizeRouterData,
        _connectors: &settings::Connectors,
    ) -> CustomResult<RequestContent, errors::ConnectorError> {
        let amount = connector_utils::convert_amount(
            self.amount_converter,
            req.request.minor_amount,
            req.request.currency,
        )?;

        let connector_router_data = bitpay::BitpayRouterData::from((amount, req));
        let connector_req = bitpay::BitpayPaymentsRequest::try_from(&connector_router_data)?;

        Ok(RequestContent::Json(Box::new(connector_req)))
    }

    fn build_request(
        &self,
        req: &types::PaymentsAuthorizeRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Post)
                .url(&types::PaymentsAuthorizeType::get_url(
                    self, req, connectors,
                )?)
                .attach_default_headers()
                .headers(types::PaymentsAuthorizeType::get_headers(
                    self, req, connectors,
                )?)
                .set_body(types::PaymentsAuthorizeType::get_request_body(
                    self, req, connectors,
                )?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsAuthorizeRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<types::PaymentsAuthorizeRouterData, errors::ConnectorError> {
        let response: bitpay::BitpayPaymentsResponse = res
            .response
            .parse_struct("Bitpay PaymentsAuthorizeResponse")
            .switch()?;
        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);
        types::RouterData::try_from(types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

impl ConnectorIntegration<api::PSync, types::PaymentsSyncData, types::PaymentsResponseData>
    for Bitpay
{
    fn get_headers(
        &self,
        req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Vec<(String, request::Maskable<String>)>, errors::ConnectorError> {
        self.build_headers(req, connectors)
    }

    fn get_content_type(&self) -> &'static str {
        self.common_get_content_type()
    }

    fn get_url(
        &self,
        req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<String, errors::ConnectorError> {
        let auth = bitpay::BitpayAuthType::try_from(&req.connector_auth_type)
            .change_context(errors::ConnectorError::FailedToObtainAuthType)?;
        let connector_id = req
            .request
            .connector_transaction_id
            .get_connector_transaction_id()
            .change_context(errors::ConnectorError::MissingConnectorTransactionID)?;
        Ok(format!(
            "{}/invoices/{}?token={}",
            self.base_url(connectors),
            connector_id,
            auth.api_key.peek(),
        ))
    }

    fn build_request(
        &self,
        req: &types::PaymentsSyncRouterData,
        connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Ok(Some(
            services::RequestBuilder::new()
                .method(services::Method::Get)
                .url(&types::PaymentsSyncType::get_url(self, req, connectors)?)
                .attach_default_headers()
                .headers(types::PaymentsSyncType::get_headers(self, req, connectors)?)
                .build(),
        ))
    }

    fn handle_response(
        &self,
        data: &types::PaymentsSyncRouterData,
        event_builder: Option<&mut ConnectorEvent>,
        res: Response,
    ) -> CustomResult<types::PaymentsSyncRouterData, errors::ConnectorError> {
        let response: bitpay::BitpayPaymentsResponse = res
            .response
            .parse_struct("bitpay PaymentsSyncResponse")
            .switch()?;
        event_builder.map(|i| i.set_response_body(&response));
        router_env::logger::info!(connector_response=?response);
        types::RouterData::try_from(types::ResponseRouterData {
            response,
            data: data.clone(),
            http_code: res.status_code,
        })
    }

    fn get_error_response(
        &self,
        res: Response,
        event_builder: Option<&mut ConnectorEvent>,
    ) -> CustomResult<ErrorResponse, errors::ConnectorError> {
        self.build_error_response(res, event_builder)
    }
}

impl ConnectorIntegration<api::Capture, types::PaymentsCaptureData, types::PaymentsResponseData>
    for Bitpay
{
    fn build_request(
        &self,
        _req: &types::PaymentsCaptureRouterData,
        _connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Err(errors::ConnectorError::FlowNotSupported {
            flow: "Capture".to_string(),
            connector: "Bitpay".to_string(),
        }
        .into())
    }
}

impl ConnectorIntegration<api::Void, types::PaymentsCancelData, types::PaymentsResponseData>
    for Bitpay
{
}

impl ConnectorIntegration<api::Execute, types::RefundsData, types::RefundsResponseData> for Bitpay {
    fn build_request(
        &self,
        _req: &types::RefundsRouterData<api::Execute>,
        _connectors: &settings::Connectors,
    ) -> CustomResult<Option<services::Request>, errors::ConnectorError> {
        Err(
            errors::ConnectorError::NotImplemented("Refund flow not Implemented".to_string())
                .into(),
        )
    }
}

impl ConnectorIntegration<api::RSync, types::RefundsData, types::RefundsResponseData> for Bitpay {
    // default implementation of build_request method will be executed
}

#[async_trait::async_trait]
impl api::IncomingWebhook for Bitpay {
    fn get_webhook_object_reference_id(
        &self,
        request: &api::IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<api::webhooks::ObjectReferenceId, errors::ConnectorError> {
        let notif: BitpayWebhookDetails = request
            .body
            .parse_struct("BitpayWebhookDetails")
            .change_context(errors::ConnectorError::WebhookReferenceIdNotFound)?;
        Ok(api_models::webhooks::ObjectReferenceId::PaymentId(
            api_models::payments::PaymentIdType::ConnectorTransactionId(notif.data.id),
        ))
    }

    fn get_webhook_event_type(
        &self,
        request: &api::IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<api::IncomingWebhookEvent, errors::ConnectorError> {
        let notif: BitpayWebhookDetails = request
            .body
            .parse_struct("BitpayWebhookDetails")
            .change_context(errors::ConnectorError::WebhookEventTypeNotFound)?;
        match notif.event.name {
            bitpay::WebhookEventType::Confirmed | bitpay::WebhookEventType::Completed => {
                Ok(api::IncomingWebhookEvent::PaymentIntentSuccess)
            }
            bitpay::WebhookEventType::Paid => {
                Ok(api::IncomingWebhookEvent::PaymentIntentProcessing)
            }
            bitpay::WebhookEventType::Declined => {
                Ok(api::IncomingWebhookEvent::PaymentIntentFailure)
            }
            bitpay::WebhookEventType::Unknown
            | bitpay::WebhookEventType::Expired
            | bitpay::WebhookEventType::Invalid
            | bitpay::WebhookEventType::Refunded
            | bitpay::WebhookEventType::Resent => Ok(api::IncomingWebhookEvent::EventNotSupported),
        }
    }

    fn get_webhook_resource_object(
        &self,
        request: &api::IncomingWebhookRequestDetails<'_>,
    ) -> CustomResult<Box<dyn masking::ErasedMaskSerialize>, errors::ConnectorError> {
        let notif: BitpayWebhookDetails = request
            .body
            .parse_struct("BitpayWebhookDetails")
            .change_context(errors::ConnectorError::WebhookEventTypeNotFound)?;
        Ok(Box::new(notif))
    }
}
