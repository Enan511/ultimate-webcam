import appIcon from "./assets/ultimate-webcam-icon.svg";
import React, { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Video, ShieldAlert, X } from "lucide-react";

interface DeviceDetectedPayload {
  serial: string;
  model_name: string;
  authorized: boolean;
}

export const UsbPromptApp: React.FC = () => {
  const [device, setDevice] = useState<DeviceDetectedPayload | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const unlistenPromise = listen<DeviceDetectedPayload>(
      "usb-device-detected",
      (event) => {
        setDevice(event.payload);
        setLoading(false);
      }
    );

    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, []);

  const handleAccept = async () => {
    if (!device) return;
    setLoading(true);
    try {
      await invoke("accept_usb_webcam", { serial: device.serial });
    } catch (err) {
      console.error("Failed to accept webcam:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleIgnore = async () => {
    if (!device) return;
    try {
      await invoke("ignore_usb_device", { serial: device.serial });
    } catch (err) {
      console.error("Failed to ignore device:", err);
    }
  };

  if (!device) {
    return (
      <div className="w-full h-full flex items-center justify-center bg-surface-50/90 text-slate-400 text-xs">
        Waiting for USB device...
      </div>
    );
  }

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0, y: 24, scale: 0.98 }}
        animate={{ opacity: 1, y: 0, scale: 1 }}
        exit={{ opacity: 0, y: 16 }}
        transition={{ type: "spring", damping: 25, stiffness: 350 }}
        className="w-full h-screen p-3 box-border bg-[#10121a]/95 backdrop-blur-xl border border-white/10 rounded-2xl shadow-2xl flex flex-col justify-between overflow-hidden"
      >
        {/* Header */}
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-gradient-to-tr from-accent/20 to-emerald-500/10 border border-accent/30 flex items-center justify-center text-accent">
              <img src={appIcon} alt="" className="w-10 h-10" />
            </div>
            <div>
              <div className="text-[11px] font-semibold tracking-wider text-accent uppercase">
                USB Connected
              </div>
              <h2 className="text-sm font-bold text-white tracking-tight line-clamp-1">
                {device.model_name || "Android Phone"}
              </h2>
            </div>
          </div>
          <button
            onClick={handleIgnore}
            className="p-1 rounded-lg text-slate-400 hover:text-white hover:bg-white/10 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Status / Notice */}
        {!device.authorized ? (
          <div className="flex items-center gap-2 p-2 rounded-lg bg-amber-500/10 border border-amber-500/20 text-amber-300 text-xs">
            <ShieldAlert className="w-4 h-4 flex-shrink-0" />
            <span>Please tap &quot;Allow USB Debugging&quot; on your phone screen.</span>
          </div>
        ) : (
          <div className="text-xs text-slate-300 leading-relaxed">
            Would you like to use this phone as a high-speed compiled USB webcam?
          </div>
        )}

        {/* Actions */}
        <div className="flex items-center gap-2 pt-1">
          <button
            onClick={handleAccept}
            disabled={loading || !device.authorized}
            className="flex-1 flex items-center justify-center gap-2 py-2 px-3 rounded-xl bg-accent hover:bg-accent-hover text-black font-semibold text-xs transition-all shadow-lg shadow-accent/20 active:scale-[0.98] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Video className="w-4 h-4" />
            {loading ? "Connecting..." : "Use as Webcam"}
          </button>
          <button
            onClick={handleIgnore}
            className="py-2 px-3 rounded-xl bg-white/5 hover:bg-white/10 border border-white/5 text-slate-300 hover:text-white font-medium text-xs transition-all active:scale-[0.98]"
          >
            Use Normally
          </button>
        </div>
      </motion.div>
    </AnimatePresence>
  );
};
