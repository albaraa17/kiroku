use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use serde::{Deserialize, Serialize};
use std::ffi::CStr;
use std::path::Path;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadParam {
    pub process_id: u32,
    pub addr: u64,
    pub ctx: u64,
    pub ctx2: u64,
}

impl ThreadParam {
    pub fn thread_key(&self) -> String {
        format!(
            "{}:{:X}:{:X}:{:X}",
            self.process_id, self.addr, self.ctx, self.ctx2
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SearchParam {
    pub pattern: [u8; 30],
    pub address_method: i32,
    pub search_method: i32,
    pub length: i32,
    pub offset: i32,
    pub search_time: i32,
    pub max_records: i32,
    pub codepage: i32,
    pub padding: i64,
    pub min_address: u64,
    pub max_address: u64,
    pub boundary_module: [u16; 120],
    pub export_module: [u16; 120],
    pub text: [u16; 30],
    pub isjithook: bool,
    pub sharememname: [u16; 64],
    pub sharememsize: u64,
}

pub type ProcessEvent = unsafe extern "C" fn(pid: u32);
pub type ThreadEventMaybeEmbed = unsafe extern "C" fn(
    hookcode: *const u16,
    hookname: *const i8,
    tp: ThreadParam,
    is_embeddable: bool,
);
pub type ThreadEvent =
    unsafe extern "C" fn(hookcode: *const u16, hookname: *const i8, tp: ThreadParam);
pub type OutputCallback = unsafe extern "C" fn(
    hookcode: *const u16,
    hookname: *const i8,
    tp: ThreadParam,
    text: *const u16,
);
pub type HostInfoHandler = unsafe extern "C" fn(info_type: i32, log: *const u16);
pub type HookInsertHandler = unsafe extern "C" fn(pid: u32, address: u64, hookcode: *const u16);
pub type EmbedCallback = unsafe extern "C" fn(text: *const u16, tp: ThreadParam);
pub type I18NQueryCallback = unsafe extern "C" fn(text: *const u16) -> *mut u16;
pub type EmuGameInfoCallback =
    unsafe extern "C" fn(id: *const u16, title: *const u16, version: *const u16);
pub type FindHooksCallback = unsafe extern "C" fn(hookcode: *const u16, text: *const u16);

type FnLunaStart = unsafe extern "C" fn(
    connect: ProcessEvent,
    disconnect: ProcessEvent,
    create: ThreadEventMaybeEmbed,
    destroy: ThreadEvent,
    output: OutputCallback,
    hostinfo: HostInfoHandler,
    hookinsert: HookInsertHandler,
    embed: EmbedCallback,
    i18n: I18NQueryCallback,
    emu_info: EmuGameInfoCallback,
);
type FnLunaSettings = unsafe extern "C" fn(
    flush_delay: i32,
    default_codepage: i32,
    max_buffer_size: i32,
    max_history_size: i32,
    enable_pc_hooks: bool,
);
type FnLunaConnectProcess = unsafe extern "C" fn(pid: u32);
type FnLunaCheckIfNeedInject = unsafe extern "C" fn(pid: u32) -> bool;
type FnLunaDetachProcess = unsafe extern "C" fn(pid: u32);
type FnLunaInsertHookCode = unsafe extern "C" fn(pid: u32, hookcode: *const u16);
type FnLunaInsertPCHooks = unsafe extern "C" fn(pid: u32, which: i32);
type FnLunaRemoveHook = unsafe extern "C" fn(pid: u32, addr: u64);
type FnLunaSyncThread = unsafe extern "C" fn(tp: ThreadParam, sync: bool);

pub struct LunaHostLibrary {
    _lib: Library,
    fn_start: FnLunaStart,
    fn_settings: FnLunaSettings,
    fn_connect_process: FnLunaConnectProcess,
    fn_check_need_inject: FnLunaCheckIfNeedInject,
    fn_detach_process: FnLunaDetachProcess,
    fn_insert_hook_code: FnLunaInsertHookCode,
    fn_insert_pc_hooks: FnLunaInsertPCHooks,
    fn_remove_hook: FnLunaRemoveHook,
    fn_sync_thread: FnLunaSyncThread,
}

impl LunaHostLibrary {
    pub fn load(dll_path: &Path) -> Result<Self> {
        unsafe {
            let lib = Library::new(dll_path)
                .with_context(|| format!("Failed to load {}", dll_path.display()))?;

            let fn_start: Symbol<FnLunaStart> = lib.get(b"Luna_Start\0")?;
            let fn_settings: Symbol<FnLunaSettings> = lib.get(b"Luna_Settings\0")?;
            let fn_connect_process: Symbol<FnLunaConnectProcess> =
                lib.get(b"Luna_ConnectProcess\0")?;
            let fn_check_need_inject: Symbol<FnLunaCheckIfNeedInject> =
                lib.get(b"Luna_CheckIfNeedInject\0")?;
            let fn_detach_process: Symbol<FnLunaDetachProcess> =
                lib.get(b"Luna_DetachProcess\0")?;
            let fn_insert_hook_code: Symbol<FnLunaInsertHookCode> =
                lib.get(b"Luna_InsertHookCode\0")?;
            let fn_insert_pc_hooks: Symbol<FnLunaInsertPCHooks> =
                lib.get(b"Luna_InsertPCHooks\0")?;
            let fn_remove_hook: Symbol<FnLunaRemoveHook> = lib.get(b"Luna_RemoveHook\0")?;
            let fn_sync_thread: Symbol<FnLunaSyncThread> = lib.get(b"Luna_SyncThread\0")?;

            Ok(Self {
                fn_start: *fn_start,
                fn_settings: *fn_settings,
                fn_connect_process: *fn_connect_process,
                fn_check_need_inject: *fn_check_need_inject,
                fn_detach_process: *fn_detach_process,
                fn_insert_hook_code: *fn_insert_hook_code,
                fn_insert_pc_hooks: *fn_insert_pc_hooks,
                fn_remove_hook: *fn_remove_hook,
                fn_sync_thread: *fn_sync_thread,
                _lib: lib,
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        connect: ProcessEvent,
        disconnect: ProcessEvent,
        create: ThreadEventMaybeEmbed,
        destroy: ThreadEvent,
        output: OutputCallback,
        hostinfo: HostInfoHandler,
        hookinsert: HookInsertHandler,
        embed: EmbedCallback,
        i18n: I18NQueryCallback,
        emu_info: EmuGameInfoCallback,
    ) {
        unsafe {
            (self.fn_start)(
                connect, disconnect, create, destroy, output, hostinfo, hookinsert, embed, i18n,
                emu_info,
            );
        }
    }

    pub fn apply_settings(
        &self,
        flush_delay: i32,
        default_codepage: i32,
        max_buffer_size: i32,
        max_history_size: i32,
        enable_pc_hooks: bool,
    ) {
        unsafe {
            (self.fn_settings)(
                flush_delay,
                default_codepage,
                max_buffer_size,
                max_history_size,
                enable_pc_hooks,
            );
        }
    }

    pub fn connect_process(&self, pid: u32) {
        unsafe {
            (self.fn_connect_process)(pid);
        }
    }

    pub fn check_if_need_inject(&self, pid: u32) -> bool {
        unsafe { (self.fn_check_need_inject)(pid) }
    }

    pub fn detach_process(&self, pid: u32) {
        unsafe {
            (self.fn_detach_process)(pid);
        }
    }

    pub fn insert_hook_code(&self, pid: u32, hookcode: &str) {
        let mut wide: Vec<u16> = hookcode.encode_utf16().collect();
        wide.push(0);
        unsafe {
            (self.fn_insert_hook_code)(pid, wide.as_ptr());
        }
    }

    pub fn insert_pc_hooks(&self, pid: u32, which: i32) {
        unsafe {
            (self.fn_insert_pc_hooks)(pid, which);
        }
    }

    pub fn remove_hook(&self, pid: u32, addr: u64) {
        unsafe {
            (self.fn_remove_hook)(pid, addr);
        }
    }

    pub fn sync_thread(&self, tp: ThreadParam, sync: bool) {
        unsafe {
            (self.fn_sync_thread)(tp, sync);
        }
    }
}

pub unsafe fn wide_ptr_to_string(ptr: *const u16) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe {
        let mut len = 0usize;
        while len < 65536 && *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        String::from_utf16_lossy(slice)
    }
}

pub unsafe fn cstr_ptr_to_string(ptr: *const i8) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_struct_layouts() {
        assert_eq!(std::mem::size_of::<ThreadParam>(), 32);
        assert_eq!(std::mem::align_of::<ThreadParam>(), 8);
        assert_eq!(std::mem::size_of::<SearchParam>(), 768);
        assert_eq!(std::mem::align_of::<SearchParam>(), 8);
    }
}
