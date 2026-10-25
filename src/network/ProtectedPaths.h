#pragma once

#include <bembeyaz/path.h>
#include <string.h>

#include <string>

// What web requests (file manager, WebDAV, plugin endpoints) and plugin manifests may not reach:
// rust/crates/bembeyaz-path/src/web.rs.
namespace protectedpaths {

// "System Volume Information" or "XTCache": never listed or touched. Up to the first NUL: a decoded
// "%00" ends the name SdFat creates.
inline bool isSystemName(const char* name, const size_t len) { return bb_path_is_system_name(name, len); }

// A name starting with a dot, or a system name.
inline bool isHiddenName(const char* name, const size_t len) { return bb_path_is_hidden_name(name, len); }

// Whether any component of the path is a hidden name. The whole len is searched, NULs included.
inline bool hasHiddenComponent(const char* path, const size_t len) { return bb_path_has_hidden_component(path, len); }

// Credential stores: their passwords are obfuscated with a device key, so the files must stay on
// the device. Mirrors the getFilePath() of WifiCredentialStore, OpdsServerStore and
// KOReaderCredentialStore; their temp/backup siblings match too. Spellings that open the same
// file ("//", "/./", "..", case, SdFat's trimming and 8.3 aliases) match as well.
inline bool isSensitivePath(const char* path) { return bb_path_is_sensitive(path, strlen(path)); }

// A plugin-supplied path (web request or device.json): absolute, no parent
// refs, not a credential store.
inline bool isPluginPath(const std::string& p) { return bb_path_is_plugin_path(p.data(), p.size()); }

}  // namespace protectedpaths
