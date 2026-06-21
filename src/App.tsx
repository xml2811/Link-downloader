import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { downloadDir } from "@tauri-apps/api/path";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Language = "en" | "es" | "pt";
type LinkStatus = "pending" | "downloading" | "completed" | "error";

type DetectedLink = {
  id: number;
  url: string;
  status: LinkStatus;
  message?: string;
  filename?: string;
};

type DownloadResult = {
  url: string;
  filename: string;
  ok: boolean;
  message: string;
};

const INTERNAL_CONCURRENCY = 10;

const translations = {
  en: {
    eyebrow: "Windows micro tool",
    title: "Bulk Link Downloader",
    subtitle:
      "Paste text with links, the app detects them automatically and downloads the files to the folder you choose.",
    pasteTitle: "Paste links",
    pasteHelp:
      "Paste direct links or GitHub release links. Use it only for files you are allowed to download.",
    detected: "links detected",
    placeholder:
      "Example:\nhttps://example.com/manual.pdf\nAnother one here: https://example.com/file.zip, and another https://example.com/image.png",
    destinationFolder: "Destination folder",
    change: "Change",
    downloadLinks: "Download links",
    downloading: "Downloading...",
    clear: "Clear",
    openFolder: "Open folder",
    foundLinks: "Found links",
    foundLinksHelp:
      "The app downloads direct links, resolves GitHub Releases to assets and avoids overwriting files with the same name.",
    noLinks: "No links detected yet.",
    completed: "Completed",
    errors: "Errors",
    file: "File",
    statusPending: "Pending",
    statusDownloading: "Downloading",
    statusCompleted: "Completed",
    statusError: "Error",
    folderError: "Could not open folder",
    downloadError: "General download error",
    language: "Language",
  },
  es: {
    eyebrow: "Microherramienta Windows",
    title: "Descargador Masivo de Links",
    subtitle:
      "Pega texto con enlaces, la app los detecta automáticamente y descarga los archivos en la carpeta que elijas.",
    pasteTitle: "Pegar links",
    pasteHelp:
      "Pega links directos o enlaces de GitHub Releases. Úsalo solo con archivos que tengas permiso para descargar.",
    detected: "links detectados",
    placeholder:
      "Ejemplo:\nhttps://example.com/manual.pdf\nAquí hay otro: https://example.com/archivo.zip, y otro https://example.com/imagen.png",
    destinationFolder: "Carpeta destino",
    change: "Cambiar",
    downloadLinks: "Descargar links",
    downloading: "Descargando...",
    clear: "Limpiar",
    openFolder: "Abrir carpeta",
    foundLinks: "Links encontrados",
    foundLinksHelp:
      "La app descarga links directos, resuelve GitHub Releases a archivos reales y evita sobrescribir archivos con el mismo nombre.",
    noLinks: "Todavía no hay links detectados.",
    completed: "Completados",
    errors: "Errores",
    file: "Archivo",
    statusPending: "Pendiente",
    statusDownloading: "Descargando",
    statusCompleted: "Completado",
    statusError: "Error",
    folderError: "No se pudo abrir la carpeta",
    downloadError: "Error general descargando",
    language: "Idioma",
  },
  pt: {
    eyebrow: "Microferramenta Windows",
    title: "Descarregador de Links em Massa",
    subtitle:
      "Cole texto com links, a aplicação detecta-os automaticamente e descarrega os ficheiros para a pasta escolhida.",
    pasteTitle: "Colar links",
    pasteHelp:
      "Cole links diretos ou links de GitHub Releases. Use apenas com ficheiros que tem permissão para descarregar.",
    detected: "links detectados",
    placeholder:
      "Exemplo:\nhttps://example.com/manual.pdf\nOutro aqui: https://example.com/ficheiro.zip, e outro https://example.com/imagem.png",
    destinationFolder: "Pasta de destino",
    change: "Alterar",
    downloadLinks: "Descarregar links",
    downloading: "A descarregar...",
    clear: "Limpar",
    openFolder: "Abrir pasta",
    foundLinks: "Links encontrados",
    foundLinksHelp:
      "A aplicação descarrega links diretos, resolve GitHub Releases para ficheiros reais e evita substituir ficheiros com o mesmo nome.",
    noLinks: "Ainda não foram detectados links.",
    completed: "Concluídos",
    errors: "Erros",
    file: "Ficheiro",
    statusPending: "Pendente",
    statusDownloading: "A descarregar",
    statusCompleted: "Concluído",
    statusError: "Erro",
    folderError: "Não foi possível abrir a pasta",
    downloadError: "Erro geral ao descarregar",
    language: "Idioma",
  },
};

function extractLinks(text: string): DetectedLink[] {
  const regex = /https?:\/\/[^\s,;'"<>]+/gi;
  const matches = text.match(regex) ?? [];

  const cleaned = matches.map((url) =>
    url
      .trim()
      .replace(/[)\].,;]+$/g, "")
  );

  const uniqueLinks = Array.from(new Set(cleaned));

  return uniqueLinks.map((url, index) => ({
    id: index + 1,
    url,
    status: "pending",
  }));
}

function App() {
  const [language, setLanguage] = useState<Language>(() => {
    const saved = localStorage.getItem("bulk-link-downloader-language");

    if (saved === "es" || saved === "en" || saved === "pt") {
      return saved;
    }

    return "en";
  });

  const t = translations[language];

  const [inputText, setInputText] = useState("");
  const [downloadFolder, setDownloadFolder] = useState("");
  const [downloadResults, setDownloadResults] = useState<Record<string, DownloadResult>>({});
  const [isDownloading, setIsDownloading] = useState(false);

  const baseLinks = useMemo(() => {
    return extractLinks(inputText);
  }, [inputText]);

  const links = useMemo(() => {
    return baseLinks.map((link) => {
      const result = downloadResults[link.url];

      if (!result) {
        return {
          ...link,
          status: isDownloading ? "downloading" as LinkStatus : "pending" as LinkStatus,
        };
      }

      return {
        ...link,
        status: result.ok ? "completed" as LinkStatus : "error" as LinkStatus,
        message: result.message,
        filename: result.filename,
      };
    });
  }, [baseLinks, downloadResults, isDownloading]);

  const completedCount = links.filter((link) => link.status === "completed").length;
  const errorCount = links.filter((link) => link.status === "error").length;
  const hasResults = completedCount > 0 || errorCount > 0;

  useEffect(() => {
    localStorage.setItem("bulk-link-downloader-language", language);
  }, [language]);

  useEffect(() => {
    async function loadDefaultFolder() {
      try {
        const downloads = await downloadDir();
        setDownloadFolder(downloads);
      } catch (error) {
        console.error("Could not get Downloads folder:", error);
        setDownloadFolder("Downloads folder");
      }
    }

    loadDefaultFolder();
  }, []);

  function handleInputChange(value: string) {
    setInputText(value);
    setDownloadResults({});
  }

  async function chooseFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title:
          language === "en"
            ? "Select download folder"
            : language === "es"
            ? "Selecciona la carpeta de descarga"
            : "Selecione a pasta de destino",
        defaultPath: downloadFolder || undefined,
      });

      if (typeof selected === "string") {
        setDownloadFolder(selected);
        setDownloadResults({});
      }
    } catch (error) {
      console.error("Folder selection error:", error);
    }
  }

  async function openDownloadFolder() {
    if (!downloadFolder) {
      return;
    }

    try {
      await invoke("open_folder", {
        folder: downloadFolder,
      });
    } catch (error) {
      console.error(`${t.folderError}:`, error);
      alert(`${t.folderError}: ${error}`);
    }
  }

  async function startDownload() {
    if (baseLinks.length === 0 || !downloadFolder || isDownloading) {
      return;
    }

    setIsDownloading(true);
    setDownloadResults({});

    try {
      const results = await invoke<DownloadResult[]>("download_files", {
        request: {
          urls: baseLinks.map((link) => link.url),
          folder: downloadFolder,
          concurrency: INTERNAL_CONCURRENCY,
        },
      });

      const mappedResults: Record<string, DownloadResult> = {};

      for (const result of results) {
        mappedResults[result.url] = result;
      }

      setDownloadResults(mappedResults);
    } catch (error) {
      console.error(`${t.downloadError}:`, error);
      alert(`${t.downloadError}: ${error}`);
    } finally {
      setIsDownloading(false);
    }
  }

  function clearAll() {
    if (isDownloading) {
      return;
    }

    setInputText("");
    setDownloadResults({});
  }

  function getStatusText(status: LinkStatus) {
    if (status === "pending") return t.statusPending;
    if (status === "downloading") return t.statusDownloading;
    if (status === "completed") return t.statusCompleted;
    return t.statusError;
  }

  return (
    <main className="app">
      <section className="hero">
        <div className="topBar">
          <div>
            <p className="eyebrow">{t.eyebrow}</p>
            <h1>{t.title}</h1>
          </div>

          <div className="languageBox">
            <label>{t.language}</label>
            <select
              value={language}
              disabled={isDownloading}
              onChange={(event) => setLanguage(event.target.value as Language)}
            >
              <option value="en">English</option>
              <option value="es">Español</option>
              <option value="pt">Português</option>
            </select>
          </div>
        </div>

        <p className="subtitle">{t.subtitle}</p>
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2>{t.pasteTitle}</h2>
            <p>{t.pasteHelp}</p>
          </div>

          <div className="counter">
            <span>{links.length}</span>
            <small>{t.detected}</small>
          </div>
        </div>

        <textarea
          value={inputText}
          disabled={isDownloading}
          onChange={(event) => handleInputChange(event.target.value)}
          placeholder={t.placeholder}
        />

        <div className="settings singleSetting">
          <div>
            <label>{t.destinationFolder}</label>
            <div className="folderRow">
              <input value={downloadFolder} readOnly />
              <button className="secondary" onClick={chooseFolder} disabled={isDownloading}>
                {t.change}
              </button>
            </div>
          </div>
        </div>

        <div className="actions">
          <button disabled={links.length === 0 || !downloadFolder || isDownloading} onClick={startDownload}>
            {isDownloading ? t.downloading : t.downloadLinks}
          </button>

          <button className="secondary" onClick={clearAll} disabled={isDownloading}>
            {t.clear}
          </button>

          <button className="secondary" onClick={openDownloadFolder} disabled={!downloadFolder}>
            {t.openFolder}
          </button>
        </div>

        {hasResults && (
          <div className="summary">
            <span>{t.completed}: {completedCount}</span>
            <span>{t.errors}: {errorCount}</span>
          </div>
        )}
      </section>

      <section className="panel">
        <div className="panelHeader">
          <div>
            <h2>{t.foundLinks}</h2>
            <p>{t.foundLinksHelp}</p>
          </div>
        </div>

        {links.length === 0 ? (
          <div className="empty">
            {t.noLinks}
          </div>
        ) : (
          <div className="linkList">
            {links.map((link) => (
              <div className="linkItem" key={link.id}>
                <span className="badge">#{link.id}</span>
                <div className="linkText">
                  <span className="url">{link.url}</span>
                  {link.filename && <small>{t.file}: {link.filename}</small>}
                  {link.message && <small>{link.message}</small>}
                </div>
                <span className={`status ${link.status}`}>
                  {getStatusText(link.status)}
                </span>
              </div>
            ))}
          </div>
        )}      </section>

      <div className="brandMark">
        MPTech Tools
      </div>
    </main>
  );
}

export default App;

