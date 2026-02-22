use crate::domain::connection::SavedConnection;
use crate::services::config::Folder;

pub fn find_folder_mut<'a>(folders: &'a mut [Folder], id: &str) -> Option<&'a mut Folder> {
    for folder in folders.iter_mut() {
        if folder.id == id {
            return Some(folder);
        }
        if let Some(f) = find_folder_mut(&mut folder.subfolders, id) {
            return Some(f);
        }
    }
    None
}

pub fn find_connection_mut<'a>(
    folders: &'a mut Vec<Folder>,
    conn_id: &str,
) -> Option<&'a mut SavedConnection> {
    for folder in folders.iter_mut() {
        if let Some(conn) = folder.connections.iter_mut().find(|c| c.id == conn_id) {
            return Some(conn);
        }
        if let Some(conn) = find_connection_mut(&mut folder.subfolders, conn_id) {
            return Some(conn);
        }
    }
    None
}

pub fn find_connection<'a>(folders: &'a [Folder], conn_id: &str) -> Option<&'a SavedConnection> {
    for folder in folders {
        if let Some(conn) = folder.connections.iter().find(|c| c.id == conn_id) {
            return Some(conn);
        }
        if let Some(conn) = find_connection(&folder.subfolders, conn_id) {
            return Some(conn);
        }
    }
    None
}

pub fn remove_connection(folders: &mut Vec<Folder>, conn_id: &str) -> Option<SavedConnection> {
    for folder in folders.iter_mut() {
        if let Some(pos) = folder.connections.iter().position(|c| c.id == conn_id) {
            return Some(folder.connections.remove(pos));
        }
        if let Some(c) = remove_connection(&mut folder.subfolders, conn_id) {
            return Some(c);
        }
    }
    None
}

pub fn set_direct_child_connection_text_tint(
    folders: &mut Vec<Folder>,
    folder_id: &str,
    tint: Option<[u8; 4]>,
) -> bool {
    if let Some(folder) = find_folder_mut(folders, folder_id) {
        for conn in &mut folder.connections {
            conn.text_tint_rgba = tint;
        }
        return true;
    }
    false
}

pub fn remove_folder(folders: &mut Vec<Folder>, folder_id: &str) -> bool {
    if let Some(pos) = folders.iter().position(|f| f.id == folder_id) {
        folders.remove(pos);
        return true;
    }
    for folder in folders.iter_mut() {
        if remove_folder(&mut folder.subfolders, folder_id) {
            return true;
        }
    }
    false
}
