//! Trait boundaries for the SQL metadata store and the content-addressed blob store.

use crate::api::{
    PublicationListRequest, PublicationListResponse, UploadCompleteRequest, UploadInitRequest,
    UploadInitResponse,
};
use crate::error::PortalError;
use crate::moderation::{QueueItem, ReportCreateRequest, ReportView};
use crate::schema::{
    AccountRow, DecisionLogRow, FileRow, PublicationRow, PublisherRow, ReportRow, TakedownRow,
    TokenRow, VersionRow,
};

/// Content-addressed object storage (S3-compatible or local folder driver).
///
/// # Rules
/// - `content_hash` is lowercase sha256 hex.
/// - `put` is idempotent: the same hash may be written twice.
///
/// # Do not
/// - Delete published objects except through a takedown workflow.
pub trait BlobStore: Send + Sync {
    fn put(&self, content_hash: &str, data: &[u8]) -> Result<(), PortalError>;
    fn get(&self, content_hash: &str) -> Result<Vec<u8>, PortalError>;
    fn has(&self, content_hash: &str) -> Result<bool, PortalError>;
}

/// SQL metadata for publishers, versions, moderation and accounts.
///
/// # Rules
/// - Publication versions are immutable once published.
/// - Yank sets a timestamp on the publication row; bytes stay addressable by hash.
pub trait PortalStore: Send + Sync {
    fn insert_publisher(&self, row: &PublisherRow) -> Result<(), PortalError>;
    fn get_publisher(&self, id: &str) -> Result<PublisherRow, PortalError>;

    fn insert_publication(&self, row: &PublicationRow) -> Result<(), PortalError>;
    fn insert_version(&self, row: &VersionRow) -> Result<(), PortalError>;
    fn list_publications(
        &self,
        query: &PublicationListRequest,
    ) -> Result<PublicationListResponse, PortalError>;

    fn record_file(&self, row: &FileRow) -> Result<(), PortalError>;

    fn begin_upload(&self, request: &UploadInitRequest) -> Result<UploadInitResponse, PortalError>;
    fn complete_upload(
        &self,
        request: &UploadCompleteRequest,
    ) -> Result<VersionRow, PortalError>;

    fn list_queue(&self, limit: u32) -> Result<Vec<QueueItem>, PortalError>;
    fn insert_report(&self, request: &ReportCreateRequest) -> Result<ReportView, PortalError>;
    fn get_report(&self, id: &str) -> Result<ReportRow, PortalError>;

    fn append_decision(&self, row: &DecisionLogRow) -> Result<(), PortalError>;
    fn insert_takedown(&self, row: &TakedownRow) -> Result<(), PortalError>;

    fn insert_account(&self, row: &AccountRow) -> Result<(), PortalError>;
    fn insert_token(&self, row: &TokenRow) -> Result<(), PortalError>;
}
