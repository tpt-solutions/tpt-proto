//! `tpt-proto-descriptor` — descriptor model and binary (de)serialization.
//!
//! Models the `google.protobuf.*` descriptor messages and provides binary
//! (de)serialization plus descriptor query APIs used by reflection, codegen,
//! and tooling.

mod types;

pub use types::*;

impl DescriptorProto {
    /// Find a regular field by name.
    pub fn find_field_by_name(&self, name: &str) -> Option<&FieldDescriptorProto> {
        self.field.iter().find(|f| f.name.as_deref() == Some(name))
    }

    /// Find a regular field by field number.
    pub fn find_field_by_number(&self, number: i32) -> Option<&FieldDescriptorProto> {
        self.field.iter().find(|f| f.number == Some(number))
    }

    /// Find a oneof by name.
    pub fn find_oneof_by_name(&self, name: &str) -> Option<&OneofDescriptorProto> {
        self.oneof_decl.iter().find(|o| o.name.as_deref() == Some(name))
    }

    /// Find a nested message by name.
    pub fn find_nested_message(&self, name: &str) -> Option<&DescriptorProto> {
        self.nested_type.iter().find(|m| m.name.as_deref() == Some(name))
    }

    /// Find a nested enum by name.
    pub fn find_nested_enum(&self, name: &str) -> Option<&EnumDescriptorProto> {
        self.enum_type.iter().find(|e| e.name.as_deref() == Some(name))
    }
}

impl EnumDescriptorProto {
    /// Find an enum value by name.
    pub fn find_value_by_name(&self, name: &str) -> Option<&EnumValueDescriptorProto> {
        self.value.iter().find(|v| v.name.as_deref() == Some(name))
    }

    /// Find an enum value by number.
    pub fn find_value_by_number(&self, number: i32) -> Option<&EnumValueDescriptorProto> {
        self.value.iter().find(|v| v.number == Some(number))
    }
}

impl FileDescriptorProto {
    /// Find a top-level message by name.
    pub fn find_message(&self, name: &str) -> Option<&DescriptorProto> {
        self.message_type.iter().find(|m| m.name.as_deref() == Some(name))
    }

    /// Find a top-level enum by name.
    pub fn find_enum(&self, name: &str) -> Option<&EnumDescriptorProto> {
        self.enum_type.iter().find(|e| e.name.as_deref() == Some(name))
    }

    /// Find a top-level service by name.
    pub fn find_service(&self, name: &str) -> Option<&ServiceDescriptorProto> {
        self.service.iter().find(|s| s.name.as_deref() == Some(name))
    }
}

impl FileDescriptorSet {
    /// Find a file by name.
    pub fn find_file(&self, name: &str) -> Option<&FileDescriptorProto> {
        self.file.iter().find(|f| f.name.as_deref() == Some(name))
    }
}
