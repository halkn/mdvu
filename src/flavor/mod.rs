pub mod azure_devops;
pub mod gfm;

#[cfg(test)]
mod tests;

use clap::ValueEnum;

use crate::markdown::model::Document;
use crate::source::SourceText;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Flavor {
    Gfm,
    #[value(name = "azure-devops")]
    AzureDevops,
}

pub fn parse(source: SourceText, flavor: Flavor) -> Document {
    match flavor {
        Flavor::Gfm => gfm::parse(source),
        Flavor::AzureDevops => azure_devops::parse(source),
    }
}
