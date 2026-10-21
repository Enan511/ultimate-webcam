import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Mic, MicOff, Volume2, Info, Radio } from "lucide-react";

interface AudioDevicePanelProps {
  isMicActive: boolean;
  onToggleMic: (active: boolean) => void;
}

export const AudioDevicePanel: React.FC<AudioDevicePanelProps> = ({
  isMicActive,
  onToggleMic,
}) => {
  const [dbLevel, setDbLevel] = useState(-90);

  useEffect(() => {
    let interval: number | undefined;

    if (isMicActive) {
      interval = window.setInterval(async () => {
        try {
          const db: number = await invoke("get_audio_db");
          setDbLevel(db);
        } catch (_err) {
          // Ignore polling errors
        }
      }, 60);
    } else {
      setDbLevel(-90);
    }

    return () => {
      if (interval) clearInterval(interval);
    };
  }, [isMicActive]);

  const handleToggle = async () => {
    const nextState = !isMicActive;
    onToggleMic(nextState);
    try {
      await invoke("set_audio", { enabled: nextState });
    } catch (err) {
      console.error("Failed to toggle audio:", err);
      onToggleMic(!nextState); // Rollback
    }
  };

  // Convert -90 dB .. 0 dB to percentage 0% .. 100%
  const clampedDb = Math.max(-60, Math.min(0, dbLevel));
  const levelPercent = Math.round(((clampedDb + 60) / 60) * 100);

  return (
    <div className="flex flex-col gap-6 p-1 text-slate-200">
      {/* Audio Stream Master Toggle */}
      <div className="flex items-center justify-between p-4 rounded-2xl bg-surface-50 border border-white/5">
        <div className="flex items-center gap-3">
          <div
            className={`w-10 h-10 rounded-xl border flex items-center justify-center transition-colors ${
              isMicActive
                ? "bg-accent/15 border-accent/30 text-accent"
                : "bg-surface-100 border-white/5 text-slate-400"
            }`}
          >
            {isMicActive ? <Mic className="w-5 h-5" /> : <MicOff className="w-5 h-5" />}
          </div>
          <div>
            <div className="text-xs font-semibold text-white">
              USB Phone Microphone
            </div>
            <div className="text-[11px] text-slate-400">
              Low-latency 48kHz 16-bit PCM streaming
            </div>
          </div>
        </div>

        <button
          onClick={handleToggle}
          className={`w-12 h-6 rounded-full transition-colors relative p-0.5 ${
            isMicActive ? "bg-accent" : "bg-surface-200"
          }`}
        >
          <div
            className={`w-5 h-5 rounded-full bg-black transition-transform ${
              isMicActive ? "translate-x-6" : "translate-x-0"
            }`}
          />
        </button>
      </div>

      {/* Audio VU Meter & Status */}
      {isMicActive && (
        <div className="flex flex-col gap-3 p-4 rounded-2xl bg-surface-50 border border-white/5 animate-in fade-in duration-300">
          <div className="flex justify-between items-center text-xs">
            <span className="text-slate-400 flex items-center gap-1.5">
              <Radio className="w-3.5 h-3.5 text-accent animate-pulse" />
              <span>Real-Time Input Level</span>
            </span>
            <span className="font-mono text-slate-200 font-semibold">
              {dbLevel > -90 ? `${dbLevel.toFixed(1)} dB` : "Silent"}
            </span>
          </div>

          {/* VU Meter Bar */}
          <div className="w-full h-3 bg-surface-200 rounded-full overflow-hidden p-0.5 flex items-center">
            <div
              className="h-full rounded-full transition-all duration-75"
              style={{
                width: `${levelPercent}%`,
                background:
                  levelPercent > 85
                    ? "linear-gradient(90deg, #00d26a 0%, #eab308 70%, #ef4444 100%)"
                    : levelPercent > 60
                    ? "linear-gradient(90deg, #00d26a 0%, #eab308 100%)"
                    : "#00d26a",
              }}
            />
          </div>

          <div className="flex justify-between text-[10px] text-slate-500 font-mono px-1">
            <span>-60 dB</span>
            <span>-30 dB</span>
            <span>-12 dB</span>
            <span>0 dB</span>
          </div>
        </div>
      )}

      {/* WASAPI Target Routing Info */}
      <div className="flex flex-col gap-3 p-4 rounded-2xl bg-surface-50 border border-white/5">
        <label className="text-xs font-semibold text-slate-400 uppercase tracking-wider flex items-center gap-2">
          <Volume2 className="w-3.5 h-3.5 text-accent" />
          <span>Windows WASAPI Routing</span>
        </label>

        <div className="p-3 rounded-xl bg-surface-100 border border-white/5 text-xs text-slate-300 flex items-start gap-2.5">
          <Info className="w-4 h-4 text-cyan-400 flex-shrink-0 mt-0.5" />
          <div className="leading-relaxed">
            Streaming audio targets <strong className="text-white">CABLE Input (VB-Audio Virtual Cable)</strong> automatically. Select <em>CABLE Output</em> in Discord, Zoom, or Teams to use your phone&apos;s microphone.
          </div>
        </div>
      </div>
    </div>
  );
};
