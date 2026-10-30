//! ## Folder-Listing Object
//!
//! The `x-obex/folder-listing` object returned by GetFolderListing (MAP sections 3.1.5
//! and 5.4), defined by OBEX section 9.1. In MAP it lists only the folders inside the
//! current folder.
//!
//! # Examples
//! ```
//! use map::folder_listing::FolderListing;
//!
//! let listing = FolderListing::parse(br#"<folder-listing version="1.0">
//!   <folder name="inbox"/>
//!   <folder name="outbox"/>
//! </folder-listing>"#).unwrap();
//!
//! assert_eq!(vec!["inbox", "outbox"], listing.names());
//! ```
//!
use crate::error::Result;
use crate::xml::{self, Node, Tag};

const OBJECT: &str = "folder-listing";

/// A folder in a [`FolderListing`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folder {
    /// The folder name, used with SetFolder and GetMessagesListing.
    pub name: String,
    /// Estimated size in bytes.
    pub size: Option<u64>,
    /// Creation time in OBEX ISO 8601 format.
    pub created: Option<String>,
    /// Last modification time in OBEX ISO 8601 format.
    pub modified: Option<String>,
    /// Last access time in OBEX ISO 8601 format.
    pub accessed: Option<String>,
}

impl Folder {
    /// Creates a folder entry with only a name.
    pub fn new(name: impl Into<String>) -> Folder {
        Folder {
            name: name.into(),
            ..Folder::default()
        }
    }
}

/// The `FolderListing` type. See [the module level documentation](index.html) for more.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FolderListing {
    /// The folders in the listed folder.
    pub folders: Vec<Folder>,
}

impl FolderListing {
    /// Creates a listing of `folders`.
    pub fn new(folders: impl IntoIterator<Item = Folder>) -> FolderListing {
        FolderListing {
            folders: folders.into_iter().collect(),
        }
    }

    /// The names of the listed folders.
    pub fn names(&self) -> Vec<&str> {
        self.folders
            .iter()
            .map(|folder| folder.name.as_str())
            .collect()
    }

    /// Parses a folder listing, ignoring file and parent-folder entries.
    pub fn parse(bytes: &[u8]) -> Result<FolderListing> {
        let nodes = xml::parse(bytes, OBJECT)?;
        let (_, children) = xml::root(&nodes, "folder-listing", OBJECT)?;

        let folders = children
            .iter()
            .filter_map(|node| match node {
                Node::Open(element) if element.name == "folder" => Some(element),
                _ => None,
            })
            .filter_map(|element| {
                Some(Folder {
                    name: element.string("name")?,
                    size: element.parsed("size"),
                    created: element.string("created"),
                    modified: element.string("modified"),
                    accessed: element.string("accessed"),
                })
            })
            .collect();

        Ok(FolderListing { folders })
    }

    /// Encodes the listing as XML.
    ///
    /// # Examples
    /// ```
    /// # use map::folder_listing::{Folder, FolderListing};
    /// let listing = FolderListing::new([Folder::new("inbox")]);
    ///
    /// assert_eq!(listing, FolderListing::parse(&listing.to_bytes()).unwrap());
    /// ```
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = String::from(xml::DECLARATION);
        out.push_str("<!DOCTYPE folder-listing SYSTEM \"obex-folder-listing.dtd\">\n");
        Tag::open(&mut out, "folder-listing")
            .attribute("version", "1.0")
            .start();

        for folder in &self.folders {
            Tag::open(&mut out, "folder")
                .attribute("name", &folder.name)
                .optional("size", folder.size)
                .optional("created", folder.created.as_deref())
                .optional("modified", folder.modified.as_deref())
                .optional("accessed", folder.accessed.as_deref())
                .empty();
        }

        xml::close(&mut out, "folder-listing");
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_obex_example_ignoring_files() {
        let listing = FolderListing::parse(
            br#"<?xml version="1.0"?>
<!DOCTYPE folder-listing SYSTEM "obex-folder-listing.dtd">
<folder-listing version="1.0">
  <parent-folder />
  <folder name = "System" created="19961103T141500Z"/>
  <folder name =" IR Inbox" created="19950330T105000Z"/>
  <file name = "Jumar.txt" created="19971209T090300Z" size="6672"/>
</folder-listing>"#,
        )
        .unwrap();

        assert_eq!(vec!["System", " IR Inbox"], listing.names());
        assert_eq!(
            Some("19961103T141500Z"),
            listing.folders[0].created.as_deref()
        );
    }

    #[test]
    fn empty_listing_round_trips() {
        let listing = FolderListing::default();
        assert_eq!(listing, FolderListing::parse(&listing.to_bytes()).unwrap());
        assert!(FolderListing::parse(b"<other/>").is_err());
    }
}
