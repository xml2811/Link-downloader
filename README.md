# Link Downloader

Link Downloader is a small portable Windows app that detects direct download links from pasted text and downloads them automatically.

Created by MPTech Tools.

## Preview

![link-downloader - Link-Downloader](docs/screenshots/Link-Downloader.png)

## Features

- Paste mixed text containing links
- Automatic link detection
- Choose destination folder
- Uses the Downloads folder by default
- Downloads multiple files
- Avoids overwriting duplicate filenames
- Opens destination folder
- English, Spanish and Portuguese interface
- Portable EXE
- No installer
- No account
- Free and open source

## Download

Portable executable:

../../releases/link-downloader/link-downloader.exe

## Development

From this folder:

tools/link-downloader

Install dependencies:

npm install

Run in development mode:

npm run tauri dev

## Build portable EXE

Run:

.\build-portable.ps1

The script builds the app and copies the final executable to:

portable/link-downloader.exe

and also to:

../../releases/link-downloader/link-downloader.exe

## Responsible use

Use this tool only for files you are allowed to download.

This app does not bypass DRM, logins, paywalls, private access controls or copyright restrictions.

## GitHub Releases support

Link Downloader can resolve GitHub release page links such as /releases/latest and download the best available asset, prioritizing portable .exe files.