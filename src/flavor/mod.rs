pub mod azure_devops;
pub mod gfm;

#[cfg(test)]
mod tests;

use crate::cli::Flavor;
use crate::markdown::model::Document;
use crate::source::SourceText;

pub fn parse(source: SourceText, flavor: Flavor) -> Document {
    match flavor {
        Flavor::Gfm => gfm::parse(source),
        Flavor::AzureDevops => azure_devops::parse(source),
    }
}
