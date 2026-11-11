//! Fixed arrays passed by value.
//!
//! A C parameter declared `uint32_t a[3]` is a pointer: the array decays at
//! the call. A store into a by-value array parameter (`fn f(mut a: [u32; 3])
//! { a[0] = 99; }`) then wrote into the caller's array, and `a = [..]` copied
//! `sizeof(pointer)` bytes into it (G33). The signature names the incoming
//! pointer `__blarr_N` (N the local id) and the body declares a real local
//! array under the parameter's name, copied from the pointer before the first
//! statement. Every read and store in the body then sees a private copy, as
//! the language's value semantics require.

use std::fmt::Write;

use crate::codegen::ir::{MirFunction, MirLocal, MirType};

use super::CBackend;

impl CBackend {
    /// The C name of the incoming pointer for a by-value array parameter.
    pub(super) fn array_param_incoming_name(param: &MirLocal) -> String {
        format!("__blarr_{}", param.id.0)
    }

    /// Whether `param` is a fixed array passed by value.
    pub(super) fn is_array_param(param: &MirLocal) -> bool {
        param.is_param && matches!(param.ty, MirType::Array(..))
    }

    /// `T (*name)[M]...` for a pointer to an array whose elements are arrays.
    /// A pointer to an array is a pointer to its first element in the C this
    /// backend writes; when that element is itself an array the declarator
    /// needs the parentheses (`uint8_t[2]* p` is not C). `None` for any
    /// other type.
    pub(super) fn nested_array_ptr_decl(&self, ty: &MirType, name: &str) -> Option<String> {
        let MirType::Ptr(inner) = ty else {
            return None;
        };
        let MirType::Array(elem, _) = inner.as_ref() else {
            return None;
        };
        if !matches!(elem.as_ref(), MirType::Array(..)) {
            return None;
        }
        Some(self.fmt_array_decl(elem, &format!("(*{name})")))
    }

    /// Declare the private copy of each by-value array parameter and fill it
    /// from the caller's array.
    pub(super) fn emit_array_param_copies(&mut self, func: &MirFunction) {
        for param in func.locals.iter().filter(|l| Self::is_array_param(l)) {
            let name = self.local_name(param.id, &func.locals);
            let incoming = Self::array_param_incoming_name(param);
            self.write_indent();
            writeln!(self.output, "{};", self.fmt_array_decl(&param.ty, &name)).unwrap();
            self.write_indent();
            writeln!(self.output, "memcpy({name}, {incoming}, sizeof({name}));").unwrap();
        }
    }
}
