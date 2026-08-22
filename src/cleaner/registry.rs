use crate::cleaner::dev_tools::{
    CargoCacheCleaner, DockerCleaner, GradleCacheCleaner, HomebrewCleaner, MavenCacheCleaner,
    NpmCacheCleaner, PipCacheCleaner,
};
use crate::cleaner::system_cache::UserCacheCleaner;
use crate::cleaner::system_logs::UserLogsCleaner;
use crate::cleaner::temp_files::TempFilesCleaner;
use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::cleaner::trash::TrashCleaner;
use crate::cleaner::xcode::{
    XcodeArchivesCleaner, XcodeDerivedDataCleaner, XcodeDeviceSupportCleaner,
};
use crate::safety::allowlist::CleanCategory;
use anyhow::Result;

pub struct CleanerRegistry {
    cleaners: Vec<Box<dyn Cleaner>>,
}

#[allow(dead_code)]
impl CleanerRegistry {
    pub fn new() -> Self {
        let cleaners: Vec<Box<dyn Cleaner>> = vec![
            Box::new(UserCacheCleaner),
            Box::new(UserLogsCleaner),
            Box::new(TempFilesCleaner),
            Box::new(XcodeDerivedDataCleaner),
            Box::new(XcodeArchivesCleaner),
            Box::new(XcodeDeviceSupportCleaner),
            Box::new(HomebrewCleaner),
            Box::new(NpmCacheCleaner),
            Box::new(PipCacheCleaner),
            Box::new(GradleCacheCleaner),
            Box::new(MavenCacheCleaner),
            Box::new(CargoCacheCleaner),
            Box::new(DockerCleaner),
            Box::new(TrashCleaner),
        ];

        Self { cleaners }
    }

    /// Retrieve all registered cleaners
    pub fn all(&self) -> &[Box<dyn Cleaner>] {
        &self.cleaners
    }

    /// Retrieve cleaners filtered by category
    #[allow(clippy::borrowed_box)]
    pub fn find_by_category(&self, category: CleanCategory) -> Option<&Box<dyn Cleaner>> {
        self.cleaners.iter().find(|c| c.category() == category)
    }

    /// Scan all or filtered cleaners and return target items grouped by cleaner
    #[allow(clippy::borrowed_box, clippy::type_complexity)]
    pub fn scan_all(
        &self,
        category_filter: Option<&[CleanCategory]>,
    ) -> Vec<(&Box<dyn Cleaner>, Result<Vec<CleanTargetItem>>)> {
        self.cleaners
            .iter()
            .filter(|c| {
                if let Some(cats) = category_filter {
                    cats.contains(&c.category())
                } else {
                    true
                }
            })
            .map(|c| (c, c.scan()))
            .collect()
    }
}
