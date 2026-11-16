//! Stock road-guidance mesh binding and its exact native placement frame.
use crate::route::point;
use anyhow::{Context, Result, ensure};
use ghostline_red::{
    assets::{AssetReader, ResourceKey},
    material::ArchiveSet,
    schema::RedSchema,
};
use glam::{DQuat, DVec3};
use serde_json::{Value, json};

pub(super) struct Glyph {
    pub mesh: String,
    pub appearance: String,
    pub rotation: DQuat,
    pub native_center: DVec3,
    pub center: DVec3,
    pub half_width: f64,
    pub auto_hide: f64,
    axis: usize,
}
impl Glyph {
    pub fn load(archives: &ArchiveSet, schema: &RedSchema) -> Result<Self> {
        let mut reader = AssetReader::new(archives, schema);
        let entity = reader.document(&ResourceKey::parse(
            r"base\quest\side_quests\sq024\entities\sq024_gps_path.ent",
        ))?;
        let components = entity["Data"]["RootChunk"]["compiledData"]["Data"]["Chunks"]
            .as_array()
            .context("stock GPS components")?;
        let meshes = components
            .iter()
            .filter(|c| c["$type"] == "entMeshComponent")
            .collect::<Vec<_>>();
        ensure!(
            meshes.len() == 1,
            "Stock GPS template must contain one mesh component"
        );
        let component = meshes[0];
        let t = &component["localTransform"];
        let mut translation = [0.; 3];
        for (i, k) in ["x", "y", "z"].iter().enumerate() {
            translation[i] = t["Position"][k]["Bits"]
                .as_f64()
                .context("GPS component position")?
                / 131072.;
        }
        let q = &t["Orientation"];
        let rotation = DQuat::from_xyzw(
            q["i"].as_f64().context("GPS rotation i")?,
            q["j"].as_f64().context("GPS rotation j")?,
            q["k"].as_f64().context("GPS rotation k")?,
            q["r"].as_f64().context("GPS rotation r")?,
        )
        .normalize();
        let mesh = component["mesh"]["DepotPath"]["$value"]
            .as_str()
            .context("stock GPS mesh path")?
            .to_owned();
        let document = reader.document(&ResourceKey::parse(&mesh))?;
        let chunks = ghostline_red::mesh::decode_geometry(&document, true)?;
        let mut low = DVec3::splat(f64::INFINITY);
        let mut high = DVec3::splat(f64::NEG_INFINITY);
        for p in chunks.iter().flat_map(|c| &c.positions) {
            let p = DVec3::new(f64::from(p[0]), -f64::from(p[2]), f64::from(p[1]));
            low = low.min(p);
            high = high.max(p);
        }
        ensure!(
            low.is_finite() && high.is_finite(),
            "Stock GPS mesh has no finite geometry"
        );
        let native_center = (low + high) / 2.;
        let center = DVec3::from_array(translation) + rotation * native_center;
        let half = (high - low) / 2.;
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let axis = (0..3)
            .max_by(|a, b| {
                (rotation * axes[*a])
                    .y
                    .abs()
                    .total_cmp(&(rotation * axes[*b]).y.abs())
            })
            .context("GPS mesh forward axis")?;
        let extents = (rotation * DVec3::X).abs() * half.x
            + (rotation * DVec3::Y).abs() * half.y
            + (rotation * DVec3::Z).abs() * half.z;
        ensure!(
            (extents.y - 2.5).abs() < 0.01 && extents.z < 0.001,
            "Stock GPS geometry is not a five-metre planar tile"
        );
        Ok(Self {
            mesh,
            appearance: component["meshAppearance"]["$value"]
                .as_str()
                .context("GPS appearance")?
                .into(),
            rotation,
            native_center,
            center,
            half_width: extents.x,
            auto_hide: component["autoHideDistance"].as_f64().unwrap_or(90.),
            axis,
        })
    }
    pub fn placement(&self, visible_center: DVec3, rotation: DQuat, length: f64) -> Value {
        let mut scale = DVec3::ONE;
        scale[self.axis] = length / 5.;
        let orientation = rotation * self.rotation;
        let origin = visible_center - orientation * (self.native_center * scale);
        json!({"mesh":self.mesh,"appearance":self.appearance,"shadow_casting":"Never","auto_hide_distance":self.auto_hide,"position":point(origin.to_array()),"node_data":{"orientation":{"i":orientation.x,"j":orientation.y,"k":orientation.z,"r":orientation.w},"scale_x":scale.x,"scale_y":scale.y,"scale_z":scale.z}})
    }
}
