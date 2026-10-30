use super::esc;
use crate::app::{App, Dialog};

pub(super) fn html(app: &App) -> String {
    let Some(dialog) = &app.dialog else {
        return String::new();
    };

    let (title,body,confirm)=match dialog {
        Dialog::Question{..}=>return String::new(),
        Dialog::NewFolder(value)=>("New folder",field("dialog",value,"Folder name","text"),"Create"),
        Dialog::Rename(value)=>("Rename",field("dialog",value,"New name","text"),"Rename"),
        Dialog::Connect(value)=>("Connect to server",format!("<p>SMB · SFTP · WebDAV · FTP</p>{}<p class=hint>Examples: smb://server/share · sftp://user@server/home/user · davs://server/files</p>",field("dialog",value,"Server address","text")),"Connect"),
        Dialog::Trash=>("Move to Trash?","<p>The selected item will be moved to the system Trash. Doria will never permanently delete it if Trash is unavailable.</p>".into(),"Move to Trash"),
        Dialog::Authenticate(_,credentials)=>("Sign in to server",format!("<p>Credentials are used for this connection only.</p>{}{}{}",field("username",&credentials.username,"Username","text"),field("domain",&credentials.domain,"Domain (optional)","text"),field("password",&credentials.password,"Password","password")),"Connect"),
    };

    format!(
        "<div class=scrim><div class=dialog><h2>{title}</h2>{body}<div class=dialog-actions><button data-action=cancel>Cancel</button><button class=primary data-action=confirm>{confirm}</button></div></div></div>"
    )
}
fn field(key: &str, value: &str, label: &str, kind: &str) -> String {
    format!(
        "<label>{label}<input type={kind} data-key={key} data-submit=dialog value='{}' aria-label='{label}' /></label>",
        esc(value)
    )
}
