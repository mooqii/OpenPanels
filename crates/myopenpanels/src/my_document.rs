//! Project-level My Documents module.
//!
//! The storage implementation is still shared with the Wiki projection while
//! the pre-1.0 database is upgraded, but callers must enter through this module
//! instead of treating the Wiki panel as the document owner.

pub use crate::wiki::{
    begin_my_document_for_target, complete_my_document_for_target, create_my_document,
    delete_my_document, finish_my_document_operation, import_my_document, list_my_documents,
    my_document_import_original, my_document_import_original_for_target, publish_my_document,
    read_my_document, recover_my_document_for_target, remove_pending_writing_document,
    rename_my_document, rename_my_document_file, reveal_my_document_import_original,
    write_my_document, write_my_document_for_agent,
};
