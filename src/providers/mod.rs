/*

SPDX-License-Identifier: AGPL-3.0-only
Copyright (c) 2025 Augustus Rizza

*/

use crate::{
    models::Entity,
    query::{EntityFilter, EntityInProvider},
};
use std::collections::HashMap;

// pub mod sec_edgar;
pub mod pyprovider;
pub mod yahoo_finance;

pub trait ProviderTrait: Send + Sync {
    fn fetch_entities(&mut self, entity: EntityInProvider) -> Result<Vec<Entity>, String>;

    /// "Smart" fetch: find cached coverage, fetch only the gaps, and return a stitched view.
    /// Implementations may choose to persist a stitched super-entity or just return the union.
    fn stitch(&mut self, filters: Vec<EntityFilter>) -> Result<Entity, String>;
}

pub struct Providers {
    list: HashMap<String, Box<dyn ProviderTrait + Send + Sync>>,
}

impl Providers {
    pub fn new() -> Self {
        Providers {
            list: HashMap::new(),
        }
    }

    pub fn provider_list(&self) -> Vec<String> {
        let mut names: Vec<String> = self.list.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn add_provider(&mut self, name: String, provider: Box<dyn ProviderTrait + Send + Sync>) {
        self.list.insert(name, provider);
    }

    pub fn get_provider(&self, name: &str) -> Option<&Box<dyn ProviderTrait + Send + Sync>> {
        self.list.get(name)
    }
    pub fn get_provider_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut Box<dyn ProviderTrait + Send + Sync>> {
        self.list.get_mut(name)
    }
}
