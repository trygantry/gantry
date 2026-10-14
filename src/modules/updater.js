// Tauri updater integration. Uses globals exposed via withGlobalTauri.
// Mirrors @tauri-apps/plugin-updater's minimal flow without needing a bundler.

const core = () => window.__TAURI__ && window.__TAURI__.core;
const dialog = () => window.__TAURI__ && window.__TAURI__.dialog;

async function checkForUpdate() {
  const c = core();
  if (!c) return null;
  const metadata = await c.invoke('plugin:updater|check', {});
  return metadata || null;
}

async function downloadAndInstall(metadata, onEvent) {
  const { invoke, Channel } = core();
  const channel = new Channel();
  if (onEvent) channel.onmessage = onEvent;
  await invoke('plugin:updater|download_and_install', {
    onEvent: channel,
    rid: metadata.rid,
  });
}

async function relaunch() {
  const c = core();
  await c.invoke('plugin:process|restart', {});
}

export async function runUpdateCheck({ silent = true } = {}) {
  try {
    const metadata = await checkForUpdate();
    if (!metadata) {
      if (!silent) {
        await dialog()?.message?.('You’re on the latest version.', {
          title: 'Gantry up to date',
          kind: 'info',
        });
      }
      return;
    }

    const ask = dialog()?.ask;
    const confirmed = ask
      ? await ask(
          `Gantry ${metadata.version} is available (you have ${metadata.currentVersion}).\n\nDownload and install now?`,
          { title: 'Update available', kind: 'info' }
        )
      : true;
    if (!confirmed) return;

    let lastPct = -1;
    await downloadAndInstall(metadata, (event) => {
      if (event.event === 'Progress' && event.data?.chunkLength && event.data?.contentLength) {
        const pct = Math.floor((event.data.downloaded / event.data.contentLength) * 100);
        if (pct !== lastPct) {
          lastPct = pct;
          console.log(`[updater] download ${pct}%`);
        }
      }
    });

    const restartNow = ask
      ? await ask('Update installed. Restart Gantry now?', {
          title: 'Restart required',
          kind: 'info',
        })
      : true;
    if (restartNow) await relaunch();
  } catch (err) {
    console.warn('[updater] check failed:', err);
    if (!silent) {
      await dialog()?.message?.(String(err), { title: 'Update check failed', kind: 'error' });
    }
  }
}
