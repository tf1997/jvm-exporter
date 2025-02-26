// use std::ffi::OsStr;
// use std::fs::File;
// use std::path::{Path, PathBuf};
// use flate2::write::GzDecoder;
// use futures::{StreamExt, TryStreamExt};
// use log::warn;
// use tar::Archive;
// use tempfile::tempdir;
// use tokio::process::Command;
// use warp::Filter;
// use zip::ZipArchive;
//
// pub async fn handle_deploy(
//     form: warp::multipart::FormData,
// ) -> Result<impl warp::Reply, warp::Rejection> {
//     let temp_dir = tempdir().map_err(|e| warp::reject::custom(DeployError::from(e)))?;
//
//     let files = save_uploaded_files(form, &temp_dir).await?;
//     let extracted_dir = extract_archive(&files[0], &temp_dir).await?;
//
//     if let Err(e) = execute_start_script(&extracted_dir).await {
//         cleanup(temp_dir);
//         return Err(warp::reject::custom(e));
//     }
//
//     cleanup(temp_dir);
//     let response = warp::http::Response::builder()
//         .body(String::from("Deployment completed successfully"));
//     Ok(response)
// }
//
// // 在handle_deploy之后添加以下实现
//
// async fn save_uploaded_files(
//     form: warp::multipart::FormData,
//     temp_dir: &tempfile::TempDir,
// ) -> Result<Vec<PathBuf>, DeployError> {
//     let mut saved_files = Vec::new();
//     let mut parts = form.into_stream();
//
//     while let Some(part) = parts.next().await {
//         let mut field = part.map_err(|e| DeployError::Io(e.into()))?;
//         let file_path = temp_dir.path().join(field.name());
//         let mut file = tokio::fs::File::create(&file_path).await?;
//
//         while let Some(chunk) = field.data().await {
//             let data = chunk.map_err(|e| DeployError::Io(e.into()))?;
//             tokio::io::copy(&mut data.as_ref(), &mut file).await?;
//         }
//         saved_files.push(file_path);
//     }
//
//     if saved_files.is_empty() {
//         Err(DeployError::Archive("No files uploaded".into()))
//     } else {
//         Ok(saved_files)
//     }
// }
//
// async fn extract_archive(
//     file_path: &Path,
//     temp_dir: &tempfile::TempDir,
// ) -> Result<PathBuf, DeployError> {
//     let extract_dir = temp_dir.path().join("extracted");
//     tokio::fs::create_dir_all(&extract_dir).await?;
//
//     match file_path.extension().and_then(OsStr::to_str) {
//         Some("zip") => {
//             let file = File::open(file_path)?;
//             let mut archive = ZipArchive::new(file)?;
//
//             for i in 0..archive.len() {
//                 let mut file = archive.by_index(i)?;
//                 let outpath = extract_dir.join(file.mangled_name());
//
//                 if file.is_dir() {
//                     tokio::fs::create_dir_all(&outpath).await?;
//                 } else {
//                     if let Some(p) = outpath.parent() {
//                         if !p.exists() {
//                             tokio::fs::create_dir_all(p).await?;
//                         }
//                     }
//                     let mut outfile = File::create(&outpath)?;
//                     std::io::copy(&mut file, &mut outfile)?;
//                 }
//             }
//         }
//         Some("gz") => {
//             let file = File::open(file_path)?;
//             let tar = GzDecoder::new(file);
//             let mut archive = Archive::new(tar);
//
//             archive.unpack(&extract_dir)?;
//         }
//         _ => return Err(DeployError::Archive("Unsupported archive format".into())),
//     }
//
//     Ok(extract_dir)
// }
//
// async fn execute_start_script(extract_dir: &Path) -> Result<(), DeployError> {
//     let candidates = &["start.sh", "start", "deploy.sh", "bootstrap"];
//     let mut script_path = None;
//
//     for entry in walkdir::WalkDir::new(extract_dir) {
//         let entry = entry?;
//         if entry.file_type().is_file() {
//             if let Some(name) = entry.file_name().to_str() {
//                 if candidates.contains(&name) {
//                     script_path = Some(entry.path().to_owned());
//                     break;
//                 }
//             }
//         }
//     }
//
//     let script = script_path.ok_or(DeployError::ScriptNotFound)?;
//
//     #[cfg(unix)] {
//         use std::os::unix::fs::PermissionsExt;
//         let mut perms = tokio::fs::metadata(&script).await?.permissions();
//         perms.set_mode(0o755);
//         tokio::fs::set_permissions(&script, perms).await?;
//     }
//
//     let output = Command::new(script)
//         .current_dir(extract_dir)
//         .output()
//         .await
//         .map_err(|e| DeployError::ExecutionFailed(e.to_string()))?;
//
//     if !output.status.success() {
//         let stderr = String::from_utf8_lossy(&output.stderr);
//         return Err(DeployError::ExecutionFailed(format!(
//             "Script failed with status {}: {}",
//             output.status, stderr
//         )));
//     }
//
//     Ok(())
// }
//
// fn cleanup(temp_dir: tempfile::TempDir) {
//     if let Err(e) = temp_dir.close() {
//         warn!("Failed to clean up temp directory: {}", e);
//     }
// }
//
// #[derive(Debug)]
// enum DeployError {
//     Io(std::io::Error),
//     Archive(String),
//     ScriptNotFound,
//     ExecutionFailed(String),
// }
//
// impl warp::reject::Reject for DeployError {}
// impl From<std::io::Error> for DeployError {
//     fn from(err: std::io::Error) -> Self {
//         DeployError::Io(err)
//     }
// }
//
// impl From<zip::result::ZipError> for DeployError {
//     fn from(err: zip::result::ZipError) -> Self {
//         DeployError::Archive(format!("ZIP error: {}", err))
//     }
// }
//
// impl From<flate2::DecompressError> for DeployError {
//     fn from(err: flate2::DecompressError) -> Self {
//         DeployError::Archive(format!("GZIP decompression error: {}", err))
//     }
// }
//
// impl From<walkdir::Error> for DeployError {
//     fn from(err: walkdir::Error) -> Self {
//         DeployError::Io(err.into())
//     }
// }