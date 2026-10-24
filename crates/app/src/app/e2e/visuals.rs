use crate::app::MultisoulApp;
use canvas::CanvasRect;
use gpui::Context;
use std::time::Instant;

#[cfg(feature = "e2e-test-support")]
mod preview;

/// 初始化动画的稳定 e2e 投影：不暴露 app 私有 MotionStore 或帧资源实现。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InitializationAnimationE2eSnapshot {
    pub kind: &'static str,
    pub asset: &'static str,
    pub frame_index: usize,
    pub neutral_visible_cells: usize,
    pub accent_visible_cells: usize,
    pub overlay_opacity_milli: u16,
    pub content_opacity_milli: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoiceWindowInsetE2eSnapshot {
    pub active: bool,
    pub opacity_milli: u16,
    pub inset_px_milli: u16,
    pub corner_seam_guard_alpha_milli: u16,
}

#[cfg(feature = "e2e-test-support")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentCardBorderE2eSnapshot {
    pub stroke_color: u32,
    pub fill_color: u32,
    pub shadow_color: u32,
    pub border_is_emphasized: bool,
    pub breathing_border_active: bool,
    pub border_glow_intensity_milli: u16,
    pub border_flash_milli: u16,
    pub shadow_count: usize,
}

#[cfg(feature = "e2e-test-support")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchReceiptEffectE2eSnapshot {
    pub canvas_active: bool,
    pub active_count: usize,
    pub pending_count: usize,
    pub unavailable_skips: u64,
    pub cancelled_effects: u64,
    pub effects: Vec<DispatchReceiptEffectItemE2eSnapshot>,
}

#[cfg(feature = "e2e-test-support")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchReceiptEffectItemE2eSnapshot {
    pub origin: &'static str,
    /// 实际播放的款式 id（meteor / fireworks …）。
    pub style: &'static str,
    pub origin_node_id: Option<u64>,
    pub target_node_id: u64,
    pub status: &'static str,
    pub phase: &'static str,
    pub progress_milli: u16,
    pub particle_count: usize,
}

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn agent_card_border_e2e_snapshot(
        &mut self,
        node_id: canvas::NodeId,
    ) -> Option<AgentCardBorderE2eSnapshot> {
        let rect = self.runtime.document.node(node_id)?.bounds;
        let visual = self
            .motion
            .resolve_node_visual_at(node_id, rect, self.theme, Instant::now());
        Some(AgentCardBorderE2eSnapshot {
            stroke_color: visual.stroke_color,
            fill_color: visual.fill_color,
            shadow_color: visual.shadow_color,
            border_is_emphasized: visual.border_is_emphasized,
            breathing_border_active: visual.breathing_border_active,
            border_glow_intensity_milli: visual
                .border_glow
                .map(|glow| (glow.intensity * 1000.0).round() as u16)
                .unwrap_or(0),
            border_flash_milli: visual
                .border_glow
                .map(|glow| (glow.flash * 1000.0).round() as u16)
                .unwrap_or(0),
            shadow_count: crate::cards::chrome::node_shadow(visual).len(),
        })
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn dispatch_receipt_effect_e2e_snapshot(&self) -> DispatchReceiptEffectE2eSnapshot {
        let counts = self.motion.dispatch_receipt_counts();
        let diagnostics = self.motion.diagnostics();
        DispatchReceiptEffectE2eSnapshot {
            canvas_active: self.motion.activity_summary().canvas_active,
            active_count: counts.active,
            pending_count: counts.pending,
            unavailable_skips: diagnostics.dispatch_unavailable_skips,
            cancelled_effects: diagnostics.dispatch_cancelled_effects,
            effects: self
                .motion
                .dispatch_receipt_presentations()
                .into_iter()
                .map(|presentation| DispatchReceiptEffectItemE2eSnapshot {
                    origin: match presentation.origin {
                        crate::motion::DispatchVisualOrigin::CommandDock => "dock",
                        crate::motion::DispatchVisualOrigin::AgentNode(_) => "agent",
                        crate::motion::DispatchVisualOrigin::IdeaRow { .. } => "idea",
                    },
                    style: presentation.style.id(),
                    origin_node_id: match presentation.origin {
                        crate::motion::DispatchVisualOrigin::CommandDock
                        | crate::motion::DispatchVisualOrigin::IdeaRow { .. } => None,
                        crate::motion::DispatchVisualOrigin::AgentNode(node_id) => Some(node_id),
                    },
                    target_node_id: presentation.target_node_id,
                    status: match presentation.status {
                        crate::motion::DispatchReceiptVisualStatus::Submitted => "submitted",
                        crate::motion::DispatchReceiptVisualStatus::Queued => "queued",
                        crate::motion::DispatchReceiptVisualStatus::Untracked => "untracked",
                    },
                    phase: match presentation.phase {
                        crate::motion::DispatchEffectPhase::Flight => "flight",
                        crate::motion::DispatchEffectPhase::Impact => "impact",
                    },
                    progress_milli: opacity_to_milli(presentation.progress),
                    particle_count: presentation.particle_count,
                })
                .collect(),
        }
    }

    /// 卡片当前实际参与渲染的矩形；canonical document bounds 可能已经先写入动画终点。
    pub fn node_visual_rect_e2e(&mut self, node_id: canvas::NodeId) -> Option<CanvasRect> {
        let canonical_rect = self.runtime.document.node(node_id)?.bounds;
        Some(
            self.motion
                .resolve_node_visual_at(node_id, canonical_rect, self.theme, Instant::now())
                .rect,
        )
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn set_node_visual_rect_for_e2e(&mut self, node_id: canvas::NodeId, rect: CanvasRect) {
        self.runtime.document.set_node_bounds(node_id, rect);
        self.motion.sync_drag_rect(node_id, rect);
    }

    pub fn initialization_e2e_snapshot(
        &self,
        node_id: canvas::NodeId,
    ) -> Option<InitializationAnimationE2eSnapshot> {
        self.motion
            .initialization_presentation(node_id)
            .map(|presentation| {
                let (neutral_visible_cells, accent_visible_cells) =
                    crate::cards::intro::intro_visible_cell_counts(
                        presentation.kind,
                        presentation.frame_index,
                    );
                InitializationAnimationE2eSnapshot {
                    kind: match presentation.kind {
                        crate::motion::InitializationAnimationKind::Terminal => "terminal",
                        crate::motion::InitializationAnimationKind::Codex => "codex",
                        crate::motion::InitializationAnimationKind::Claude => "claude",
                        crate::motion::InitializationAnimationKind::Cursor => "cursor",
                        crate::motion::InitializationAnimationKind::Grok => "grok",
                        crate::motion::InitializationAnimationKind::OpenCode => "opencode",
                        crate::motion::InitializationAnimationKind::InfCode => "infcode",
                        crate::motion::InitializationAnimationKind::Kimi => "kimi",
                        crate::motion::InitializationAnimationKind::ZCode => "zcode",
                        crate::motion::InitializationAnimationKind::Pi => "pi",
                        crate::motion::InitializationAnimationKind::Hermes => "hermes",
                    },
                    asset: crate::cards::intro::intro_asset_name(presentation.kind),
                    frame_index: presentation.frame_index,
                    neutral_visible_cells,
                    accent_visible_cells,
                    overlay_opacity_milli: opacity_to_milli(presentation.overlay_opacity),
                    content_opacity_milli: opacity_to_milli(presentation.content_opacity),
                }
            })
    }

    pub fn begin_assistant_playback_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.voice_window_inset_events.push(
            crate::app::voice_window_inset::TtsPlaybackSource::Assistant,
            crate::app::voice_window_inset::TtsPlaybackEvent::Started,
        );
        let _ = cx;
    }

    /// 注入一块已成功排入播放时间线的助手 PCM，供 headless e2e 验证可见能量反馈。
    pub fn queue_assistant_pcm_for_e2e(
        &mut self,
        chunk: audio::PcmAudioChunk,
        cx: &mut Context<Self>,
    ) {
        self.voice_window_inset_events.push(
            crate::app::voice_window_inset::TtsPlaybackSource::Assistant,
            crate::app::voice_window_inset::TtsPlaybackEvent::PcmQueued(
                crate::app::voice_window_inset::PcmEnergyTimeline::from_pcm(&chunk),
            ),
        );
        let _ = cx;
    }

    pub fn finish_assistant_playback_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.voice_window_inset_events.push(
            crate::app::voice_window_inset::TtsPlaybackSource::Assistant,
            crate::app::voice_window_inset::TtsPlaybackEvent::Finished,
        );
        let _ = cx;
    }

    pub fn voice_window_inset_e2e_snapshot(&self) -> VoiceWindowInsetE2eSnapshot {
        let snapshot = self.voice_window_inset_events.snapshot();
        match snapshot.visual {
            Some(visual) => VoiceWindowInsetE2eSnapshot {
                active: true,
                opacity_milli: opacity_to_milli(visual.opacity),
                inset_px_milli: (visual.inset_px * 1000.0).round() as u16,
                corner_seam_guard_alpha_milli: opacity_to_milli(snapshot.corner_seam_guard_alpha),
            },
            None => VoiceWindowInsetE2eSnapshot {
                active: false,
                opacity_milli: 0,
                inset_px_milli: 0,
                corner_seam_guard_alpha_milli: 0,
            },
        }
    }
}

fn opacity_to_milli(opacity: f32) -> u16 {
    (opacity.clamp(0.0, 1.0) * 1000.0).round() as u16
}
