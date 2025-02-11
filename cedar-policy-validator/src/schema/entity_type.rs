/*
 * Copyright Cedar Contributors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! This module contains the definition of `ValidatorEntityType`

use nonempty::NonEmpty;
use serde::Serialize;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashSet};

use cedar_policy_core::{ast::EntityType, transitive_closure::TCNode};

use crate::types::{AttributeType, Attributes, OpenTag, Type};

#[cfg(feature = "protobufs")]
use crate::proto;

#[cfg(feature = "protobufs")]
use cedar_policy_core::ast;

/// Contains entity type information for use by the validator. The contents of
/// the struct are the same as the schema entity type structure, but the
/// `member_of` relation is reversed to instead be `descendants`.
#[derive(Clone, Debug, Serialize)]
pub struct ValidatorEntityType {
    /// The name of the entity type.
    pub(crate) name: EntityType,

    /// The set of entity types that can be members of this entity type. When
    /// this structure is initially constructed, the field will contain direct
    /// children, but it will be updated to contain the closure of all
    /// descendants before it is used in any validation.
    pub descendants: HashSet<EntityType>,

    /// The kind of entity type: enumerated and standard
    pub kind: ValidatorEntityTypeKind,
}

#[derive(Clone, Debug, Serialize)]
pub struct StandardValidatorEntityType {
    /// The attributes associated with this entity.
    pub(crate) attributes: Attributes,

    /// Indicates that this entity type may have additional attributes
    /// other than the declared attributes that may be accessed under partial
    /// schema validation. We do not know if they are present, and do not know
    /// their type when they are present. Attempting to access an undeclared
    /// attribute under standard validation is an error regardless of this flag.
    pub(crate) open_attributes: OpenTag,

    /// Tag type for this entity type. `None` indicates that entities of this
    /// type are not allowed to have tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tags: Option<Type>,
}

/// The kind of validator entity types
/// It can either be a standard (non-enum) entity type, or
/// an enumerated entity type
#[derive(Clone, Debug, Serialize)]
pub enum ValidatorEntityTypeKind {
    /// Standard, aka non-enum
    Standard(StandardValidatorEntityType),
    /// Enumerated
    Enum(NonEmpty<SmolStr>),
}

impl ValidatorEntityType {
    /// Return `true` if this entity type has an [`EntityType`] declared as a
    /// possible descendant in the schema.
    pub fn has_descendant_entity_type(&self, ety: &EntityType) -> bool {
        self.descendants.contains(ety)
    }

    /// An iterator over the attributes of this entity
    pub fn attributes(&self) -> Attributes {
        match &self.kind {
            ValidatorEntityTypeKind::Enum(_) => Attributes {
                attrs: BTreeMap::new(),
            },
            ValidatorEntityTypeKind::Standard(ty) => Attributes::with_attributes(
                ty.attributes()
                    .map(|(key, value)| (key.clone(), value.clone())),
            ),
        }
    }

    /// Get the type of the attribute with the given name, if it exists
    pub fn attr(&self, attr: &str) -> Option<&AttributeType> {
        match &self.kind {
            ValidatorEntityTypeKind::Enum(_) => None,
            ValidatorEntityTypeKind::Standard(ty) => ty.attributes.get_attr(attr),
        }
    }

    /// Get the open attributes
    pub fn open_attributes(&self) -> OpenTag {
        match &self.kind {
            ValidatorEntityTypeKind::Enum(_) => OpenTag::ClosedAttributes,
            ValidatorEntityTypeKind::Standard(ty) => ty.open_attributes,
        }
    }

    /// Get the type of tags on this entity. `None` indicates that entities of
    /// this type are not allowed to have tags.
    pub fn tag_type(&self) -> Option<&Type> {
        match &self.kind {
            ValidatorEntityTypeKind::Enum(_) => None,
            ValidatorEntityTypeKind::Standard(ty) => ty.tag_type(),
        }
    }
}

impl StandardValidatorEntityType {
    /// An iterator over the attributes of this entity
    pub fn attributes(&self) -> impl Iterator<Item = (&SmolStr, &AttributeType)> {
        self.attributes.iter()
    }

    /// Get the type of tags on this entity. `None` indicates that entities of
    /// this type are not allowed to have tags.
    pub fn tag_type(&self) -> Option<&Type> {
        self.tags.as_ref()
    }
}

impl TCNode<EntityType> for ValidatorEntityType {
    fn get_key(&self) -> EntityType {
        self.name.clone()
    }

    fn add_edge_to(&mut self, k: EntityType) {
        self.descendants.insert(k);
    }

    fn out_edges(&self) -> Box<dyn Iterator<Item = &EntityType> + '_> {
        Box::new(self.descendants.iter())
    }

    fn has_edge_to(&self, e: &EntityType) -> bool {
        self.descendants.contains(e)
    }
}

#[cfg(feature = "protobufs")]
impl From<&ValidatorEntityType> for proto::ValidatorEntityType {
    fn from(v: &ValidatorEntityType) -> Self {
        let tags = v.tag_type().map(|tags| proto::Tag {
            optional_type: Some(proto::Type::from(tags)),
        });
        let enums: Vec<String> = match &v.kind {
            ValidatorEntityTypeKind::Enum(choices) => {
                choices.into_iter().map(|s| s.to_string()).collect()
            }
            ValidatorEntityTypeKind::Standard(_) => vec![],
        };
        Self {
            name: Some(ast::proto::EntityType::from(&v.name)),
            descendants: v
                .descendants
                .iter()
                .map(ast::proto::EntityType::from)
                .collect(),
            attributes: Some(proto::Attributes::from(&v.attributes())),
            open_attributes: proto::OpenTag::from(&v.open_attributes()).into(),
            tags,
            enums,
        }
    }
}

#[cfg(feature = "protobufs")]
impl From<&proto::ValidatorEntityType> for ValidatorEntityType {
    // PANIC SAFETY: experimental feature
    #[allow(clippy::expect_used)]
    fn from(v: &proto::ValidatorEntityType) -> Self {
        let name = ast::EntityType::from(
            v.name
                .as_ref()
                .expect("`as_ref()` for field that should exist"),
        );
        let descendants = v.descendants.iter().map(ast::EntityType::from).collect();
        Self {
            name,
            descendants,
            // We use emptiness of the `enums` field as an indictor to tell if `v` represents an enumerated entity type or not
            // In other words, we essentially ignore other fields like `attributes` when `enums` is not empty
            kind: match &v.enums[..] {
                [] => {
                    let tags = match &v.tags {
                        Some(tags) => tags.optional_type.as_ref().map(Type::from),
                        None => None,
                    };
                    ValidatorEntityTypeKind::Standard(StandardValidatorEntityType {
                        attributes: Attributes::from(
                            v.attributes
                                .as_ref()
                                .expect("`as_ref()` for field that should exist"),
                        ),
                        open_attributes: OpenTag::from(
                            &proto::OpenTag::try_from(v.open_attributes)
                                .expect("decode should succeed"),
                        ),
                        tags,
                    })
                }
                enums => ValidatorEntityTypeKind::Enum(
                    NonEmpty::collect(enums.iter().map(SmolStr::new))
                        .expect("enums should be nonempty"),
                ),
            },
        }
    }
}
