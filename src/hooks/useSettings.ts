import { useEffect, useState } from 'react';
import { load, Store } from '@tauri-apps/plugin-store';

let storePromise: Promise<Store> | null = null;

function getStore(): Promise<Store> {
  if (!storePromise) {
    storePromise = load('settings.json', { autoSave: true, defaults: {} });
  }
  return storePromise;
}

function makeSetter<T>(setState: (v: T) => void, key: string) {
  return async (value: T) => {
    setState(value);
    const store = await getStore();
    await store.set(key, value);
  };
}

export function useSettings() {
  const [pollInterval, setPollIntervalState] = useState(2000);
  const [scanMode, setScanModeState] = useState(false);
  const [alwaysOnTop, setAlwaysOnTopState] = useState(false);
  const [devMode, setDevModeState] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    getStore().then(async store => {
      const pi = await store.get<number>('pollInterval');
      const sm = await store.get<boolean>('scanMode');
      const aot = await store.get<boolean>('alwaysOnTop');
      const dm = await store.get<boolean>('devMode');
      if (pi != null) setPollIntervalState(pi);
      if (sm != null) setScanModeState(sm);
      if (aot != null) setAlwaysOnTopState(aot);
      if (dm != null) setDevModeState(dm);
      setLoaded(true);
    }).catch(err => {
      console.error('useSettings: store failed to load, using defaults:', err);
      setLoaded(true);
    });
  }, []);

  return {
    pollInterval,
    setPollInterval: makeSetter(setPollIntervalState, 'pollInterval'),
    scanMode,
    setScanMode: makeSetter(setScanModeState, 'scanMode'),
    alwaysOnTop,
    setAlwaysOnTop: makeSetter(setAlwaysOnTopState, 'alwaysOnTop'),
    devMode,
    setDevMode: makeSetter(setDevModeState, 'devMode'),
    loaded,
  };
}
