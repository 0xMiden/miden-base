use alloc::format;

use miden_protocol::asset::{Asset, AssetClass, AssetComposition, AssetId};

use super::{MessageDecodeExt, required};
use crate::{ConversionError, ConversionResultExt, proto};

impl From<&AssetClass> for proto::asset::AssetClass {
    fn from(asset_class: &AssetClass) -> Self {
        Self {
            suffix: Some(asset_class.suffix().into()),
            prefix: Some(asset_class.prefix().into()),
        }
    }
}

impl From<AssetClass> for proto::asset::AssetClass {
    fn from(asset_class: AssetClass) -> Self {
        Self::from(&asset_class)
    }
}

fn decode_asset_version(version: i32) -> Result<(), ConversionError> {
    match proto::asset::AssetVersion::try_from(version) {
        Ok(proto::asset::AssetVersion::V1) => Ok(()),
        Ok(proto::asset::AssetVersion::Unspecified) => {
            Err(ConversionError::message("asset id version is unspecified"))
        },
        Err(error) => Err(ConversionError::with_source(
            format!("unknown asset id version {version}"),
            error,
        )),
    }
}

impl From<&AssetId> for proto::asset::AssetId {
    fn from(asset_id: &AssetId) -> Self {
        use proto::asset::asset_id::Composition;

        let composition = match asset_id.composition() {
            AssetComposition::None => Composition::NonFungible(asset_id.asset_class().into()),
            AssetComposition::Fungible => Composition::Fungible(()),
            AssetComposition::Custom => Composition::Custom(asset_id.asset_class().into()),
        };
        Self {
            version: proto::asset::AssetVersion::V1 as i32,
            faucet_id: Some(asset_id.faucet_id().into()),
            composition: Some(composition),
        }
    }
}

impl From<AssetId> for proto::asset::AssetId {
    fn from(asset_id: AssetId) -> Self {
        Self::from(&asset_id)
    }
}

impl TryFrom<proto::asset::AssetId> for AssetId {
    type Error = ConversionError;

    fn try_from(message: proto::asset::AssetId) -> Result<Self, Self::Error> {
        use proto::asset::asset_id::Composition;

        decode_asset_version(message.version).context("version")?;

        let decoder = message.decoder();
        let faucet_id = required!(decoder, message.faucet_id)?;
        match message.composition {
            Some(Composition::Fungible(())) => Ok(Self::new_fungible(faucet_id)),
            Some(Composition::NonFungible(class)) => Self::new(
                class.try_into().context("composition.non_fungible")?,
                faucet_id,
                AssetComposition::None,
            )
            .map_err(ConversionError::new),
            Some(Composition::Custom(class)) => Self::new(
                class.try_into().context("composition.custom")?,
                faucet_id,
                AssetComposition::Custom,
            )
            .map_err(ConversionError::new),
            None => Err(ConversionError::missing_field::<proto::asset::AssetId>("composition")),
        }
    }
}

impl From<&Asset> for proto::asset::Asset {
    fn from(asset: &Asset) -> Self {
        Self {
            asset_id: Some(asset.id().into()),
            value: Some(asset.to_value_word().into()),
        }
    }
}

impl From<Asset> for proto::asset::Asset {
    fn from(asset: Asset) -> Self {
        Self::from(&asset)
    }
}

impl TryFrom<proto::asset::Asset> for Asset {
    type Error = ConversionError;

    fn try_from(message: proto::asset::Asset) -> Result<Self, Self::Error> {
        let decoder = message.decoder();
        let asset_id = required!(decoder, message.asset_id)?;
        let value = required!(decoder, message.value)?;

        Self::new(asset_id, value).map_err(ConversionError::new)
    }
}
