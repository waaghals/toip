use directories::ProjectDirs;
use crate::metadata::{NAME, ORGANIZATION, QUALIFIER};

pub fn project_directories() -> ProjectDirs {
    ProjectDirs::from(QUALIFIER, ORGANIZATION, NAME).unwrap()
}
