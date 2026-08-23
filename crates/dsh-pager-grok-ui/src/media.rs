//! Capability-gated media and external-link presentation.

use dsh_pager_render::TerminalCapabilities;

use crate::host_adapter::CapabilityMatrix;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    InlineFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDescriptor {
    pub kind: MediaKind,
    pub attachment_id: Option<String>,
    pub media_type: Option<String>,
    pub name: Option<String>,
    pub width: Option<u16>,
    pub height: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaRender {
    Inline {
        label: String,
        width: u16,
        height: u16,
    },
    Placeholder {
        label: String,
        reason: MediaFallback,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaFallback {
    CapabilityUnavailable,
    InvalidDimensions,
    MissingAttachment,
}

impl std::fmt::Display for MediaFallback {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::CapabilityUnavailable => "inline image capability unavailable",
            Self::InvalidDimensions => "media dimensions are invalid",
            Self::MissingAttachment => "media attachment is missing",
        })
    }
}

pub fn render_media(
    descriptor: &MediaDescriptor,
    terminal: TerminalCapabilities,
    host: CapabilityMatrix,
) -> MediaRender {
    let label = descriptor
        .name
        .as_deref()
        .or(descriptor.media_type.as_deref())
        .or(descriptor.attachment_id.as_deref())
        .unwrap_or("image")
        .to_string();
    let width = descriptor.width.unwrap_or(1);
    let height = descriptor.height.unwrap_or(1);
    if descriptor.attachment_id.is_none() && descriptor.name.is_none() {
        return MediaRender::Placeholder {
            label,
            reason: MediaFallback::MissingAttachment,
        };
    }
    if width == 0 || height == 0 {
        return MediaRender::Placeholder {
            label,
            reason: MediaFallback::InvalidDimensions,
        };
    }
    if !terminal.alternate_screen || !terminal.cell_diff || !host.image {
        return MediaRender::Placeholder {
            label,
            reason: MediaFallback::CapabilityUnavailable,
        };
    }
    MediaRender::Inline {
        label,
        width: width.min(terminal_width_limit(terminal)),
        height: height.min(terminal_height_limit(terminal)),
    }
}

fn terminal_width_limit(_terminal: TerminalCapabilities) -> u16 {
    // The actual viewport is applied by the caller's layout. Keep a stable
    // upper bound here so a hostile host payload cannot resize the frame.
    160
}

fn terminal_height_limit(_terminal: TerminalCapabilities) -> u16 {
    80
}

pub fn placeholder_text(render: &MediaRender) -> String {
    match render {
        MediaRender::Inline {
            label,
            width,
            height,
        } => {
            format!("[image: {label} {width}x{height}]")
        }
        MediaRender::Placeholder { label, reason } => format!("[image: {label} ({reason})]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal() -> TerminalCapabilities {
        TerminalCapabilities {
            alternate_screen: true,
            bracketed_paste: true,
            mouse: true,
            osc52: true,
            hyperlinks: true,
            cursor: true,
            cell_diff: true,
        }
    }

    #[test]
    fn unsupported_image_is_explicit_placeholder() {
        let descriptor = MediaDescriptor {
            kind: MediaKind::Image,
            attachment_id: Some("img-1".into()),
            media_type: Some("image/png".into()),
            name: None,
            width: Some(2),
            height: Some(3),
        };
        let render = render_media(&descriptor, terminal(), CapabilityMatrix::default());
        assert!(matches!(
            render,
            MediaRender::Placeholder {
                reason: MediaFallback::CapabilityUnavailable,
                ..
            }
        ));
    }

    #[test]
    fn supported_image_has_bounded_dimensions() {
        let descriptor = MediaDescriptor {
            kind: MediaKind::Image,
            attachment_id: Some("img-1".into()),
            media_type: None,
            name: Some("plot".into()),
            width: Some(999),
            height: Some(999),
        };
        let host = CapabilityMatrix {
            image: true,
            ..CapabilityMatrix::default()
        };
        assert!(matches!(
            render_media(&descriptor, terminal(), host),
            MediaRender::Inline {
                width: 160,
                height: 80,
                ..
            }
        ));
    }
}
