//! Base AST node definitions.
//!
//! Defines the `Node` trait with `Location`.
use core::fmt;
use std::fmt::{Display, Formatter};
use std::{any::Any, cmp::Reverse};

use crate::node_kind::NodeKind;

#[derive(Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Location {
    pub offset_start: u32,
    pub offset_end: u32,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub source: String,
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "Location {{ offset_start: {}, offset_end: {}, start_line: {}, start_column: {}, end_line: {}, end_column: {}, source: {} }}",
            self.offset_start, self.offset_end, self.start_line, self.start_column, self.end_line, self.end_column, self.source
        )
    }
}

#[allow(dead_code)]
pub trait Node: Any + std::fmt::Debug {
    fn id(&self) -> u32;
    fn location(&self) -> Location;
    fn node_type_name(&self) -> String {
        std::any::type_name::<Self>()
            .split("::")
            .last()
            .unwrap_or_default()
            .to_string()
    }
    fn children(&self) -> Vec<NodeKind>;
    fn sorted_children(&self) -> Vec<NodeKind> {
        let mut children = self.children();
        children.sort_by_key(|c| Reverse(c.id()));
        children
    }
}

#[allow(dead_code)]
#[derive(Clone, Default, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Mutability {
    #[default]
    Immutable,
    Mutable,
    Constant,
}

#[macro_export]
macro_rules! ast_enum {
    (
        $(#[$outer:meta])*
        $enum_vis:vis enum $name:ident {
            $(
                $(#[$arm_attr:meta])*
                $(@$conv:ident)? $arm:ident $( ( $($tuple:tt)* ) )? $( { $($struct:tt)* } )? ,
            )*
        }
    ) => {
        $(#[$outer])*
        #[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
        $enum_vis enum $name {
            $(
                $(#[$arm_attr])*
                $arm $( ( $($tuple)* ) )? $( { $($struct)* } )? ,
            )*
        }

        impl $name {
            #[must_use]
            $enum_vis fn id(&self) -> u32 {
                match self {
                    $(
                        $name::$arm(ref _n) => { ast_enum!(@id_arm _n, $( $conv )?) }
                    )*
                }
            }

            #[must_use]
            $enum_vis fn location(&self) -> $crate::node::Location {
                match self {
                    $(
                        $name::$arm(ref _n) => { ast_enum!(@location_arm _n, $( $conv )?) }
                    )*
                }
            }

            #[must_use]
            pub fn children(&self) -> Vec<$crate::node_kind::NodeKind> {
                match self {
                    $(
                        $name::$arm(_a) => {
                            _a.children()
                        }
                    )*
                }
            }
        }
    };

    (@id_arm $inner:ident, ) => {
        $inner.id()
    };

    (@location_arm $inner:ident, ) => {
        $inner.location()
    };
}

#[macro_export]
macro_rules! ast_enums {
    (
        $(
            $(#[$outer:meta])*
            $enum_vis:vis enum $name:ident {
                $(
                    $(#[$arm_attr:meta])*
                    $(@$conv:ident)? $arm:ident $( ( $($tuple:tt)* ) )? $( { $($struct:tt)* } )? ,
                )*
            }
        )+
    ) => {
        $(
            $crate::ast_enum! {
                $(#[$outer])*
                $enum_vis enum $name {
                    $(
                        $(#[$arm_attr:meta])*
                        $(@$conv)? $arm $( ( $($tuple)* ) )? $( { $($struct)* } )? ,
                    )*
                }
            }
        )+
    };
}

#[macro_export]
macro_rules! ast_node {
    (
        $(#[$outer:meta])*
        $struct_vis:vis struct $name:ident {
            $(
                $(#[$field_attr:meta])*
                $field_vis:vis $field_name:ident : $field_ty:ty
            ),* $(,)?
        }
    ) => {
        $(#[$outer])*
        #[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
        $struct_vis struct $name {
            pub id: u32,
            pub location: $crate::node::Location,
            $(
                $(#[$field_attr])*
                $field_vis $field_name : $field_ty,
            )*
        }
    };
}

#[macro_export]
macro_rules! ast_nodes {
    (
        $(
            $(#[$outer:meta])*
            $struct_vis:vis struct $name:ident { $($fields:tt)* }
        )+
    ) => {
        $(
            $crate::ast_node! {
                $(#[$outer])*
                $struct_vis struct $name { $($fields)* }
            }
        )+
    };
}

#[macro_export]
macro_rules! ast_node_impl {
    (
        $(#[$outer:meta])*
        impl Node for $name:ident {
            $(
                $(#[$method_attr:meta])*
                fn $method:ident ( $($args:tt)* ) -> $ret:ty $body:block
            )*
        }
    ) => {
        $(#[$outer])*
        impl Node for $name {
            fn id(&self) -> u32 {
                self.id
            }

            fn location(&self) -> $crate::node::Location {
                self.location.clone()
            }

            $(
                $(#[$method_attr])*
                fn $method ( $($args)* ) -> $ret $body
            )*
        }
    };
}

#[macro_export]
macro_rules! ast_nodes_impl {
    (
        $(
            $(#[$outer:meta])*
            impl Node for $name:ident {
                $(
                    $(#[$method_attr:meta])*
                    fn $method:ident ( $($args:tt)* ) -> $ret:ty $body:block
                )*
            }
        )+
    ) => {
        $(
            $crate::ast_node_impl! {
                $(#[$outer])*
                impl Node for $name {
                    $(
                        $(#[$method_attr])*
                        fn $method ( $($args)* ) -> $ret $body
                    )*
                }
            }
        )+
    };
}
