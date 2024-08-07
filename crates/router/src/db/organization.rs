use common_utils::{errors::CustomResult, id_type};
use diesel_models::organization as storage;
use error_stack::report;
use router_env::{instrument, tracing};

use crate::{connection, core::errors, services::Store};

#[async_trait::async_trait]
pub trait OrganizationInterface {
    async fn insert_organization(
        &self,
        organization: storage::OrganizationNew,
    ) -> CustomResult<storage::Organization, errors::StorageError>;

    async fn find_organization_by_org_id(
        &self,
        org_id: &id_type::OrganizationId,
    ) -> CustomResult<storage::Organization, errors::StorageError>;

    async fn update_organization_by_org_id(
        &self,
        org_id: &id_type::OrganizationId,
        update: storage::OrganizationUpdate,
    ) -> CustomResult<storage::Organization, errors::StorageError>;
}

#[async_trait::async_trait]
impl OrganizationInterface for Store {
    #[instrument(skip_all)]
    async fn insert_organization(
        &self,
        organization: storage::OrganizationNew,
    ) -> CustomResult<storage::Organization, errors::StorageError> {
        let conn = connection::pg_connection_write(self).await?;
        organization
            .insert(&conn)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }

    #[instrument(skip_all)]
    async fn find_organization_by_org_id(
        &self,
        org_id: &id_type::OrganizationId,
    ) -> CustomResult<storage::Organization, errors::StorageError> {
        let conn = connection::pg_connection_read(self).await?;
        storage::Organization::find_by_org_id(&conn, org_id.to_owned())
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }

    #[instrument(skip_all)]
    async fn update_organization_by_org_id(
        &self,
        org_id: &id_type::OrganizationId,
        update: storage::OrganizationUpdate,
    ) -> CustomResult<storage::Organization, errors::StorageError> {
        let conn = connection::pg_connection_write(self).await?;

        storage::Organization::update_by_org_id(&conn, org_id.to_owned(), update)
            .await
            .map_err(|error| report!(errors::StorageError::from(error)))
    }
}

#[async_trait::async_trait]
impl OrganizationInterface for super::MockDb {
    async fn insert_organization(
        &self,
        organization: storage::OrganizationNew,
    ) -> CustomResult<storage::Organization, errors::StorageError> {
        let mut organizations = self.organizations.lock().await;

        if organizations
            .iter()
            .any(|org| org.org_id == organization.org_id)
        {
            Err(errors::StorageError::DuplicateValue {
                entity: "org_id",
                key: None,
            })?
        }
        let org = storage::Organization {
            org_id: organization.org_id.clone(),
            org_name: organization.org_name,
            organization_details: organization.organization_details,
            metadata: organization.metadata,
            created_at: common_utils::date_time::now(),
            modified_at: common_utils::date_time::now(),
        };
        organizations.push(org.clone());
        Ok(org)
    }

    async fn find_organization_by_org_id(
        &self,
        org_id: &id_type::OrganizationId,
    ) -> CustomResult<storage::Organization, errors::StorageError> {
        let organizations = self.organizations.lock().await;

        organizations
            .iter()
            .find(|org| org.org_id == *org_id)
            .cloned()
            .ok_or(
                errors::StorageError::ValueNotFound(format!(
                    "No organization available for org_id = {:?}",
                    org_id
                ))
                .into(),
            )
    }

    async fn update_organization_by_org_id(
        &self,
        org_id: &id_type::OrganizationId,
        update: storage::OrganizationUpdate,
    ) -> CustomResult<storage::Organization, errors::StorageError> {
        let mut organizations = self.organizations.lock().await;

        organizations
            .iter_mut()
            .find(|org| org.org_id == *org_id)
            .map(|org| match &update {
                storage::OrganizationUpdate::Update {
                    org_name,
                    organization_details,
                    metadata,
                } => storage::Organization {
                    org_name: org_name.clone(),
                    organization_details: organization_details.clone(),
                    metadata: metadata.clone(),
                    ..org.to_owned()
                },
            })
            .ok_or(
                errors::StorageError::ValueNotFound(format!(
                    "No organization available for org_id = {:?}",
                    org_id
                ))
                .into(),
            )
    }
}
