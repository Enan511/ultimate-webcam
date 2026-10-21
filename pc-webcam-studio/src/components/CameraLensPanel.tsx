import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Camera,
  Layers,
  ZoomIn,
  Sun,
  Flashlight,
  Sliders,
  CheckCircle2,
} from "lucide-react";

export interface CameraMode {
  width: number;
  height: number;
  fps: number;
}

export interface CameraDescriptor {
  id: string;
  name: string;
  lensFacing: string;
  focalLength: number;
  aperture: number;
  hasFlash: boolean;
  opticalStabilization: boolean;
  physicalIds: string[];
  modes: CameraMode[];
  zoomRange: { min: number; max: number };
}

export interface CapabilitiesManifest {
  type: string;
  model: string;
  manufacturer: string;
  soc: string;
  cameras: CameraDescriptor[];
  audioSupported: boolean;
  maxSupportedBitrate: number;
}

interface CameraLensPanelProps {
  manifest: CapabilitiesManifest | null;
  activeCameraId: string;
  onSelectCamera: (cameraId: string) => void;
  isTorchOn: boolean;
  onToggleTorch: () => void;
}

export const CameraLensPanel: React.FC<CameraLensPanelProps> = ({
  manifest,
  activeCameraId,
  onSelectCamera,
  isTorchOn,
  onToggleTorch,
}) => {
  const activeCamera =
    manifest?.cameras.find((c) => c.id === activeCameraId) ||
    manifest?.cameras[0];

  const [selectedModeIndex, setSelectedModeIndex] = useState(0);
  const [bitrateMbps, setBitrateMbps] = useState(25);
  const [zoomRatio, setZoomRatio] = useState(1.0);
  const [exposureCompensation, setExposureCompensation] = useState(0);

  const handleApplyConfig = async (
    cameraId: string,
    mode: CameraMode,
    bitrate: number
  ) => {
    try {
      await invoke("configure_camera", {
        config: {
          cmd: "CONFIGURE",
          cameraId,
          width: mode.width,
          height: mode.height,
          fps: mode.fps,
          bitrate: bitrate * 1_000_000,
          codec: "h264",
        },
      });
    } catch (err) {
      console.error("Failed to configure camera:", err);
    }
  };

  const handleCameraChange = (cameraId: string) => {
    onSelectCamera(cameraId);
    const cam = manifest?.cameras.find((c) => c.id === cameraId);
    if (cam && cam.modes.length > 0) {
      setSelectedModeIndex(0);
      handleApplyConfig(cameraId, cam.modes[0], bitrateMbps);
    }
  };

  const handleModeChange = (index: number) => {
    setSelectedModeIndex(index);
    if (activeCamera && activeCamera.modes[index]) {
      handleApplyConfig(activeCamera.id, activeCamera.modes[index], bitrateMbps);
    }
  };

  const handleZoomChange = async (ratio: number) => {
    setZoomRatio(ratio);
    try {
      await invoke("set_zoom", { ratio });
    } catch (err) {
      console.error("Failed to set zoom:", err);
    }
  };

  const handleExposureChange = async (compensation: number) => {
    setExposureCompensation(compensation);
    try {
      await invoke("set_exposure", { compensation });
    } catch (err) {
      console.error("Failed to set exposure compensation:", err);
    }
  };

  const handleBitrateCommit = (newBitrate: number) => {
    setBitrateMbps(newBitrate);
    if (activeCamera && activeCamera.modes[selectedModeIndex]) {
      handleApplyConfig(
        activeCamera.id,
        activeCamera.modes[selectedModeIndex],
        newBitrate
      );
    }
  };

  return (
    <div className="flex flex-col gap-6 p-1 text-slate-200">
      {/* Device Badge */}
      <div className="flex items-center justify-between p-3 rounded-2xl bg-surface-50 border border-white/5">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-accent/15 border border-accent/30 flex items-center justify-center text-accent">
            <Camera className="w-4 h-4" />
          </div>
          <div>
            <div className="text-xs font-semibold text-white">
              {manifest ? `${manifest.manufacturer} ${manifest.model}` : "Searching USB..."}
            </div>
            <div className="text-[11px] text-slate-400">
              {manifest ? `Chipset: ${manifest.soc}` : "Connecting to port 27183"}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-1.5 px-2.5 py-1 rounded-full bg-accent/10 border border-accent/20 text-accent text-[11px] font-semibold">
          <CheckCircle2 className="w-3.5 h-3.5" />
          <span>Active</span>
        </div>
      </div>

      {/* Available Lenses */}
      <div>
        <label className="text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2.5 flex items-center gap-2">
          <Layers className="w-3.5 h-3.5 text-accent" />
          <span>Hardware Sensor Selection</span>
        </label>
        <div className="grid grid-cols-1 gap-2">
          {manifest?.cameras.map((cam) => {
            const isSelected = cam.id === activeCamera?.id;
            return (
              <button
                key={cam.id}
                onClick={() => handleCameraChange(cam.id)}
                className={`flex items-center justify-between p-3 rounded-xl border text-left transition-all ${
                  isSelected
                    ? "bg-accent/15 border-accent text-white shadow-lg shadow-accent/10"
                    : "bg-surface-50 hover:bg-surface-100 border-white/5 text-slate-300"
                }`}
              >
                <div>
                  <div className="text-xs font-semibold">{cam.name}</div>
                  <div className="text-[11px] text-slate-400">
                    Facing: {cam.lensFacing} • OIS: {cam.opticalStabilization ? "Yes" : "No"}
                  </div>
                </div>
                {isSelected && (
                  <span className="w-2 h-2 rounded-full bg-accent animate-ping" />
                )}
              </button>
            );
          })}
        </div>
      </div>

      {/* Supported Hardware Modes (Resolution & FPS) */}
      {activeCamera && (
        <div>
          <label className="text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2.5 flex items-center gap-2">
            <Sliders className="w-3.5 h-3.5 text-accent" />
            <span>Resolution & Frame Rate</span>
          </label>
          <div className="grid grid-cols-2 gap-2">
            {activeCamera.modes.map((mode, idx) => {
              const isSelected = idx === selectedModeIndex;
              return (
                <button
                  key={`${mode.width}x${mode.height}@${mode.fps}`}
                  onClick={() => handleModeChange(idx)}
                  className={`p-2.5 rounded-xl border text-center transition-all ${
                    isSelected
                      ? "bg-accent/20 border-accent text-white font-semibold"
                      : "bg-surface-50 hover:bg-surface-100 border-white/5 text-slate-300"
                  }`}
                >
                  <div className="text-xs">
                    {mode.width === 3840
                      ? "4K UHD"
                      : mode.width === 1920
                      ? "1080p FHD"
                      : `${mode.width}×${mode.height}`}
                  </div>
                  <div className="text-[11px] text-slate-400">{mode.fps} FPS</div>
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* Sliders: Bitrate, Zoom, Exposure */}
      <div className="flex flex-col gap-4 p-4 rounded-2xl bg-surface-50 border border-white/5">
        {/* Bitrate */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400">USB Bitrate Target</span>
            <span className="font-semibold text-accent">{bitrateMbps} Mbps</span>
          </div>
          <input
            type="range"
            min={5}
            max={50}
            step={1}
            value={bitrateMbps}
            onChange={(e) => setBitrateMbps(Number(e.target.value))}
            onMouseUp={(e) => handleBitrateCommit(Number((e.target as HTMLInputElement).value))}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Zoom */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <ZoomIn className="w-3.5 h-3.5" />
              <span>Hardware Zoom</span>
            </span>
            <span className="font-semibold text-white">{zoomRatio.toFixed(1)}x</span>
          </div>
          <input
            type="range"
            min={activeCamera?.zoomRange.min || 1.0}
            max={activeCamera?.zoomRange.max || 10.0}
            step={0.1}
            value={zoomRatio}
            onChange={(e) => handleZoomChange(Number(e.target.value))}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Exposure Compensation */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <Sun className="w-3.5 h-3.5" />
              <span>Exposure Compensation</span>
            </span>
            <span className="font-semibold text-white">
              {exposureCompensation > 0 ? `+${exposureCompensation}` : exposureCompensation} EV
            </span>
          </div>
          <input
            type="range"
            min={-4}
            max={4}
            step={1}
            value={exposureCompensation}
            onChange={(e) => handleExposureChange(Number(e.target.value))}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Rear Torch Switch */}
        {activeCamera?.hasFlash && (
          <div className="flex items-center justify-between pt-2 border-t border-white/5">
            <div className="flex items-center gap-2 text-xs">
              <Flashlight className={`w-4 h-4 ${isTorchOn ? "text-amber-400" : "text-slate-400"}`} />
              <span>Rear LED Flash / Torch</span>
            </div>
            <button
              onClick={onToggleTorch}
              className={`w-11 h-6 rounded-full transition-colors relative p-0.5 ${
                isTorchOn ? "bg-amber-400" : "bg-surface-200"
              }`}
            >
              <div
                className={`w-5 h-5 rounded-full bg-white transition-transform ${
                  isTorchOn ? "translate-x-5" : "translate-x-0"
                }`}
              />
            </button>
          </div>
        )}
      </div>
    </div>
  );
};
