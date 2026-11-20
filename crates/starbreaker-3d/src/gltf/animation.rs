//! Animation clips (`clip::Clip`) → glTF animations, one per clip.
//!
//! - Bone tracks drive the node of the same (case-insensitive) name: rotation
//!   (`[w, x, y, z]` → glTF `[x, y, z, w]`) and translation, linear. Animated
//!   nodes are switched from `matrix` to TRS, as glTF requires. Vehicle
//!   and interior clips resolve names within their rig instances.
//! - Morph tracks drive `weights` of every mesh node whose mesh carries any
//!   of the targets; the mesh's targets are sampled at the union of the
//!   tracks' times (a target without a track holds 0).
//! - Material parameter tracks are written to the animation's extras
//!   (`starbreaker_material_params`): glTF cannot animate shader parameters.
//!
//! A clip that resolves to no channel is dropped with a warning (glTF
//! animations need one); an invalid clip is an error.

use std::collections::HashMap;
use std::ops::Range;

use gltf_json as json;
use json::validation::Checked;

use super::GlbBuilder;
use crate::clip::Clip;
use crate::error::Error;

/// One decoded rig's clips and the scene node ranges instantiated from that rig.
pub(crate) struct RigAnimation {
    pub clips: Vec<(Clip, serde_json::Value)>,
    pub targets: Vec<RigTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RigTarget {
    Root,
    Child(usize),
    Interior(usize),
}

impl GlbBuilder {
    /// Add one glTF animation per unscoped clip (character and mesh writers).
    pub(crate) fn add_animations(&mut self, clips: &[Clip]) -> Result<(), Error> {
        for clip in clips {
            self.add_animation(clip, None, serde_json::Map::new())?;
        }
        Ok(())
    }

    /// Resolve names independently in each instance of this rig.
    pub(crate) fn add_rig_animations(
        &mut self,
        clips: Vec<(Clip, serde_json::Value)>,
        ranges: &[Range<usize>],
    ) -> Result<(), Error> {
        let scopes: Vec<_> = ranges.iter().map(|range| self.rig_names(range.clone())).collect();
        if scopes.is_empty() {
            return Ok(());
        }
        for (clip, metadata) in clips {
            self.add_animation(&clip, Some(&scopes), metadata.as_object().cloned().unwrap_or_default())?;
        }
        Ok(())
    }

    fn add_animation(
        &mut self,
        clip: &Clip,
        scopes: Option<&[HashMap<String, u32>]>,
        mut extras: serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), Error> {
        if let Some(bad) = clip.invalid_track() {
            return Err(Error::Other(format!("animation '{}': invalid {bad}", clip.name)));
        }
        let times = clip.bones.iter().flat_map(|track| {
            track.rotation.iter().map(|keys| keys.times.as_slice())
                .chain(track.translation.iter().map(|keys| keys.times.as_slice()))
        }).chain(clip.morphs.iter().map(|track| track.keys.times.as_slice()));
        for keys in times {
            if !keys.iter().all(|key| key.is_finite() && *key >= 0.0) ||
                !keys.windows(2).all(|pair| pair[0] < pair[1])
            {
                return Err(Error::Other(format!("animation '{}': glTF key times must be finite, non-negative and strictly increasing", clip.name)));
            }
        }

        let mut channels = Vec::new();
        let mut samplers = Vec::new();
        let mut add = |builder: &mut Self, node: u32, path, times: &[f32], output: Vec<f32>, type_| {
            let input = builder.time_accessor(times);
            let output = builder.float_accessor(&output, type_);
            let sampler = samplers.len() as u32;
            samplers.push(json::animation::Sampler {
                input: json::Index::new(input),
                interpolation: Checked::Valid(json::animation::Interpolation::Linear),
                output: json::Index::new(output),
                extensions: None,
                extras: Default::default(),
            });
            channels.push(json::animation::Channel {
                sampler: json::Index::new(sampler),
                target: json::animation::Target {
                    node: json::Index::new(node),
                    path: Checked::Valid(path),
                    extensions: None,
                    extras: Default::default(),
                },
                extensions: None,
                extras: Default::default(),
            });
        };

        for track in &clip.bones {
            let bone = track.bone.to_lowercase();
            let maps = scopes.unwrap_or_else(|| std::slice::from_ref(&self.node_name_to_idx));
            let nodes: Vec<u32> = maps.iter().filter_map(|scope| scope.get(&bone).copied()).collect();
            if nodes.is_empty() {
                log::debug!("animation '{}': no node for bone '{}'", clip.name, track.bone);
            }
            for node in nodes {
                self.use_trs(node);
                if let Some(keys) = &track.rotation {
                    let out = aligned_xyzw(&keys.values);
                    add(self, node, json::animation::Property::Rotation, &keys.times, out, json::accessor::Type::Vec4);
                }
                if let Some(keys) = &track.translation {
                    let out = keys.values.iter().flatten().copied().collect();
                    add(self, node, json::animation::Property::Translation, &keys.times, out, json::accessor::Type::Vec3);
                }
            }
        }

        if !clip.morphs.is_empty() {
            for node in 0..self.nodes_json.len() as u32 {
                let Some(mesh) = self.nodes_json[node as usize].mesh else { continue };
                let Some(targets) = self.morph_names.get(&(mesh.value() as u32)).cloned() else { continue };
                let tracks: Vec<_> = targets.iter().map(|t| clip.morphs.iter().find(|m| &m.name == t)).collect();
                if tracks.iter().all(Option::is_none) {
                    continue;
                }
                let mut times: Vec<f32> = tracks.iter().flatten().flat_map(|m| m.keys.times.iter().copied()).collect();
                times.sort_by(f32::total_cmp);
                times.dedup();
                let out = times
                    .iter()
                    .flat_map(|&t| tracks.iter().map(move |m| m.map_or(0.0, |m| m.keys.sample(t))))
                    .collect();
                add(self, node, json::animation::Property::MorphTargetWeights, &times, out, json::accessor::Type::Scalar);
            }
        }

        if channels.is_empty() {
            log::warn!("animation '{}' moves nothing in this scene; not written", clip.name);
            return Ok(());
        }
        if !clip.material_params.is_empty() {
            let params: Vec<serde_json::Value> = clip
                .material_params
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "shader": p.shader,
                        "param": p.param,
                        "info": p.info,
                        "times": p.keys.times,
                        "values": p.keys.values,
                    })
                })
                .collect();
            extras.insert("starbreaker_material_params".into(), serde_json::Value::Array(params));
        }
        let extras = (!extras.is_empty()).then(|| serde_json::value::to_raw_value(&extras));
        self.animations_json.push(json::Animation {
            name: Some(clip.name.clone()),
            channels,
            samplers,
            extensions: None,
            extras: extras.transpose().map_err(|e| Error::Other(e.to_string()))?,
        });
        Ok(())
    }

    /// A SCALAR f32 accessor of key times, with the min/max glTF requires.
    fn time_accessor(&mut self, times: &[f32]) -> u32 {
        let index = self.float_accessor(times, json::accessor::Type::Scalar);
        let accessor = &mut self.accessors[index as usize];
        accessor.min = Some(serde_json::json!([times.first().copied().unwrap_or(0.0)]));
        accessor.max = Some(serde_json::json!([times.last().copied().unwrap_or(0.0)]));
        index
    }

    fn float_accessor(&mut self, values: &[f32], type_: json::accessor::Type) -> u32 {
        let width = match type_ {
            json::accessor::Type::Vec4 => 4,
            json::accessor::Type::Vec3 => 3,
            _ => 1,
        };
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let offset = self.push_aligned(&bytes);
        self.push_accessor(offset, bytes.len(), values.len() / width, json::accessor::ComponentType::F32, type_, None, None)
    }

    /// Replace a node's `matrix` with the equivalent TRS (animated nodes must
    /// not use `matrix`).
    fn use_trs(&mut self, node: u32) {
        let node = &mut self.nodes_json[node as usize];
        let Some(matrix) = node.matrix.take() else { return };
        let (scale, rotation, translation) = glam::Mat4::from_cols_array(&matrix).to_scale_rotation_translation();
        node.translation = Some(translation.to_array());
        node.rotation = Some(json::scene::UnitQuaternion(rotation.to_array()));
        node.scale = Some(scale.to_array());
    }
}

/// glTF stores xyzw. Keep successive keys in one hemisphere for linear interpolation.
fn aligned_xyzw(keys: &[[f32; 4]]) -> Vec<f32> {
    let mut previous = glam::Quat::IDENTITY;
    keys.iter().enumerate().flat_map(|(index, &[w, x, y, z])| {
        let mut value = glam::Quat::from_xyzw(x, y, z, w);
        if index > 0 && previous.dot(value) < 0.0 {
            value = -value;
        }
        previous = value;
        value.to_array()
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::{BoneTrack, Keys};

    #[test]
    fn gltf_rotation_keys_stay_in_the_same_hemisphere() {
        let mut builder = GlbBuilder::new();
        builder.nodes_json.push(json::Node {
            name: Some("synthetic_joint".into()),
            ..Default::default()
        });
        builder.node_name_to_idx.insert("synthetic_joint".into(), 0);
        let clip = Clip {
            name: "rotate".into(),
            bones: vec![BoneTrack {
                bone: "synthetic_joint".into(),
                rotation: Some(Keys {
                    times: vec![0.0, 1.0],
                    values: vec![[1.0, 0.0, 0.0, 0.0], [-1.0, 0.0, 0.0, 0.0]],
                }),
                translation: None,
            }],
            ..Default::default()
        };

        builder.add_animations(&[clip]).unwrap();

        let output = builder.animations_json[0].samplers[0].output.value();
        let accessor = &builder.accessors[output];
        let byte_offset = builder.buffer_views[accessor.buffer_view.unwrap().value()].byte_offset.unwrap().0 as usize;
        let values: Vec<_> = builder.bin[byte_offset..byte_offset + 32]
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(&values[..4], &values[4..]);
    }
    #[test]
    fn duplicate_joint_names_resolve_to_the_clip_rig() {
        let mut builder = GlbBuilder::new();
        for _ in 0..3 {
            builder.nodes_json.push(json::Node {
                name: Some("synthetic_joint".into()),
                ..Default::default()
            });
        }
        builder.node_name_to_idx.insert("synthetic_joint".into(), 0);
        let clip = Clip {
            name: "synthetic_child/move".into(),
            bones: vec![BoneTrack {
                bone: "Synthetic_Joint".into(),
                rotation: None,
                translation: Some(Keys {
                    times: vec![0.0, 0.5],
                    values: vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                }),
            }],
            ..Default::default()
        };

        builder.add_rig_animations(
            vec![(clip, serde_json::json!({"fps": 30, "frame_count": 15, "source": "caf"}))],
            &[1..2, 2..3],
        ).unwrap();

        let animations = &builder.animations_json;
        assert_eq!(animations.len(), 1);
        let target_nodes: Vec<_> = animations[0].channels.iter().map(|c| c.target.node.value()).collect();
        assert_eq!(target_nodes, [1, 2]);
        let metadata: serde_json::Value = serde_json::from_str(animations[0].extras.as_ref().unwrap().get()).unwrap();
        assert_eq!(metadata["frame_count"], 15);
    }
    #[test]
    fn trs_conversion_keeps_world_bind_transform() {
        let mut builder = GlbBuilder::new();
        let parent = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::splat(2.0),
            glam::Quat::from_rotation_y(0.5),
            glam::Vec3::new(2.0, 3.0, 4.0),
        );
        builder.nodes_json.push(json::Node {
            matrix: Some(parent.to_cols_array()),
            children: Some(vec![json::Index::new(1)]),
            ..Default::default()
        });
        builder.nodes_json.push(json::Node {
            matrix: Some(glam::Mat4::from_translation(glam::Vec3::X).to_cols_array()),
            ..Default::default()
        });
        let before = glam::Mat4::from_cols_array(&builder.compute_node_world_matrix(1));
        builder.use_trs(0);
        let after = glam::Mat4::from_cols_array(&builder.compute_node_world_matrix(1));
        assert!(before.abs_diff_eq(after, 1e-6));
    }
}
