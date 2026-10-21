import React, { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Sparkles,
  SlidersHorizontal,
  SunMedium,
  Contrast,
  Palette,
  Thermometer,
  Zap,
  CircleDot,
  RotateCcw,
} from "lucide-react";

export interface FilterSettings {
  brightness: number;
  contrast: number;
  saturation: number;
  warmth: number;
  sharpness: number;
  vignette: number;
  lut_preset: number;
  mirror_h: number;
  flip_v: number;
  rotation: number;
  zoom_crop: number;
  width: number;
  height: number;
  _pad0: number;
  _pad1: number;
  _pad2: number;
}

const DEFAULT_FILTERS: FilterSettings = {
  brightness: 0.0,
  contrast: 1.0,
  saturation: 1.0,
  warmth: 0.0,
  sharpness: 0.0,
  vignette: 0.0,
  lut_preset: 0,
  mirror_h: 0,
  flip_v: 0,
  rotation: 0,
  zoom_crop: 1.0,
  width: 1920.0,
  height: 1080.0,
  _pad0: 0.0,
  _pad1: 0.0,
  _pad2: 0.0,
};

const LUT_PRESETS = [
  { id: 0, name: "Natural", desc: "Clean pass-through", color: "from-slate-700 to-slate-900" },
  { id: 1, name: "Studio Warm", desc: "Pleasing skin tones", color: "from-amber-600 to-orange-950" },
  { id: 2, name: "Crisp B&W", desc: "High-contrast mono", color: "from-neutral-400 to-neutral-900" },
  { id: 3, name: "Teal & Orange", desc: "Hollywood cinema", color: "from-teal-600 to-amber-700" },
  { id: 4, name: "Vintage Sepia", desc: "Warm nostalgic glow", color: "from-yellow-700 to-amber-950" },
  { id: 5, name: "Contrast Noir", desc: "Deep dark shadows", color: "from-zinc-500 to-black" },
  { id: 6, name: "Soft Glow", desc: "Dreamy diffused light", color: "from-rose-500 to-purple-950" },
];

interface FilterStudioPanelProps {
  filterSettings: FilterSettings;
  onUpdateFilters: (filters: FilterSettings) => void;
}

export const FilterStudioPanel: React.FC<FilterStudioPanelProps> = ({
  filterSettings,
  onUpdateFilters,
}) => {
  const [filters, setFilters] = useState<FilterSettings>(filterSettings);

  const applyUniforms = async (updated: FilterSettings) => {
    setFilters(updated);
    onUpdateFilters(updated);
    try {
      await invoke("update_filter_uniforms", { uniforms: updated });
    } catch (err) {
      console.error("Failed to update GPU filter uniforms:", err);
    }
  };

  const handleLutSelect = (presetId: number) => {
    applyUniforms({ ...filters, lut_preset: presetId });
  };

  const handleReset = () => {
    applyUniforms({
      ...DEFAULT_FILTERS,
      mirror_h: filters.mirror_h,
      flip_v: filters.flip_v,
      rotation: filters.rotation,
    });
  };

  return (
    <div className="flex flex-col gap-6 p-1 text-slate-200">
      {/* Header with Reset */}
      <div className="flex items-center justify-between">
        <label className="text-xs font-semibold text-slate-400 uppercase tracking-wider flex items-center gap-2">
          <Sparkles className="w-3.5 h-3.5 text-accent" />
          <span>Cinematic LUT Presets</span>
        </label>
        <button
          onClick={handleReset}
          className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-surface-50 hover:bg-surface-100 border border-white/5 text-[11px] text-slate-400 hover:text-white transition-colors"
        >
          <RotateCcw className="w-3 h-3" />
          <span>Reset Defaults</span>
        </button>
      </div>

      {/* LUT Cards Grid */}
      <div className="grid grid-cols-2 gap-2">
        {LUT_PRESETS.map((preset) => {
          const isSelected = filters.lut_preset === preset.id;
          return (
            <button
              key={preset.id}
              onClick={() => handleLutSelect(preset.id)}
              className={`p-3 rounded-xl border text-left transition-all relative overflow-hidden group ${
                isSelected
                  ? "border-accent bg-accent/10 shadow-lg shadow-accent/10"
                  : "border-white/5 bg-surface-50 hover:bg-surface-100"
              }`}
            >
              <div
                className={`absolute top-0 right-0 w-16 h-16 rounded-bl-full bg-gradient-to-br ${preset.color} opacity-40 group-hover:opacity-60 transition-opacity`}
              />
              <div className="relative z-10">
                <div className="text-xs font-semibold text-white">{preset.name}</div>
                <div className="text-[10px] text-slate-400 mt-0.5">{preset.desc}</div>
              </div>
            </button>
          );
        })}
      </div>

      {/* GPU Shader Sliders */}
      <div className="flex flex-col gap-4 p-4 rounded-2xl bg-surface-50 border border-white/5">
        <div className="text-xs font-semibold text-slate-400 uppercase tracking-wider mb-1 flex items-center gap-2">
          <SlidersHorizontal className="w-3.5 h-3.5 text-accent" />
          <span>DirectX 12 Shader Adjustments</span>
        </div>

        {/* Brightness */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <SunMedium className="w-3.5 h-3.5" />
              <span>Brightness</span>
            </span>
            <span className="font-semibold text-white">
              {filters.brightness > 0 ? `+${filters.brightness.toFixed(2)}` : filters.brightness.toFixed(2)}
            </span>
          </div>
          <input
            type="range"
            min={-0.5}
            max={0.5}
            step={0.01}
            value={filters.brightness}
            onChange={(e) => applyUniforms({ ...filters, brightness: Number(e.target.value) })}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Contrast */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <Contrast className="w-3.5 h-3.5" />
              <span>Contrast</span>
            </span>
            <span className="font-semibold text-white">{filters.contrast.toFixed(2)}x</span>
          </div>
          <input
            type="range"
            min={0.5}
            max={1.8}
            step={0.02}
            value={filters.contrast}
            onChange={(e) => applyUniforms({ ...filters, contrast: Number(e.target.value) })}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Saturation */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <Palette className="w-3.5 h-3.5" />
              <span>Saturation</span>
            </span>
            <span className="font-semibold text-white">{filters.saturation.toFixed(2)}x</span>
          </div>
          <input
            type="range"
            min={0.0}
            max={2.0}
            step={0.05}
            value={filters.saturation}
            onChange={(e) => applyUniforms({ ...filters, saturation: Number(e.target.value) })}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Color Warmth / Temperature */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <Thermometer className="w-3.5 h-3.5" />
              <span>Color Temperature (Warmth)</span>
            </span>
            <span className="font-semibold text-white">
              {filters.warmth > 0 ? `+${filters.warmth.toFixed(2)}` : filters.warmth.toFixed(2)}
            </span>
          </div>
          <input
            type="range"
            min={-1.0}
            max={1.0}
            step={0.02}
            value={filters.warmth}
            onChange={(e) => applyUniforms({ ...filters, warmth: Number(e.target.value) })}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Sharpness (3x3 Unsharp Mask) */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <Zap className="w-3.5 h-3.5" />
              <span>Adaptive Sharpness</span>
            </span>
            <span className="font-semibold text-white">{filters.sharpness.toFixed(2)}</span>
          </div>
          <input
            type="range"
            min={0.0}
            max={1.5}
            step={0.05}
            value={filters.sharpness}
            onChange={(e) => applyUniforms({ ...filters, sharpness: Number(e.target.value) })}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>

        {/* Radial Vignette */}
        <div>
          <div className="flex justify-between text-xs mb-1.5">
            <span className="text-slate-400 flex items-center gap-1.5">
              <CircleDot className="w-3.5 h-3.5" />
              <span>Radial Vignette</span>
            </span>
            <span className="font-semibold text-white">
              {Math.round(filters.vignette * 100)}%
            </span>
          </div>
          <input
            type="range"
            min={0.0}
            max={1.0}
            step={0.02}
            value={filters.vignette}
            onChange={(e) => applyUniforms({ ...filters, vignette: Number(e.target.value) })}
            className="w-full h-1.5 bg-surface-200 rounded-lg appearance-none cursor-pointer accent-accent"
          />
        </div>
      </div>
    </div>
  );
};
