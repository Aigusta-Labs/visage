use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use visage_core::{CosineMatcher, Embedding, FaceModel, MatchResult, Matcher};
use visage_hw::Camera;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("camera error: {0}")]
    Camera(#[from] visage_hw::CameraError),
    #[error("detector error: {0}")]
    Detector(#[from] visage_core::detector::DetectorError),
    #[error("recognizer error: {0}")]
    Recognizer(#[from] visage_core::recognizer::RecognizerError),
    #[error("no face detected in any captured frame")]
    NoFaceDetected,
    #[error("engine thread exited")]
    ChannelClosed,
}

/// Result of an enrollment operation.
pub struct EnrollResult {
    pub embedding: Embedding,
    pub quality_score: f32,
}

/// Result of a verification operation.
pub struct VerifyResult {
    pub result: MatchResult,
    /// Reserved for v3: surface capture quality metadata to callers without a schema change.
    #[allow(dead_code)]
    pub best_quality: f32,
}

/// Messages sent from D-Bus handlers to the engine thread.
enum EngineRequest {
    Enroll {
        frames_count: usize,
        reply: oneshot::Sender<Result<EnrollResult, EngineError>>,
    },
    Verify {
        gallery: Vec<FaceModel>,
        threshold: f32,
        frames_count: usize,
        reply: oneshot::Sender<Result<VerifyResult, EngineError>>,
    },
}

/// Clone-safe handle to the engine thread.
#[derive(Clone)]
pub struct EngineHandle {
    tx: mpsc::Sender<EngineRequest>,
}

impl EngineHandle {
    /// Request enrollment: capture frames, detect best face, extract embedding.
    pub async fn enroll(&self, frames_count: usize) -> Result<EnrollResult, EngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(EngineRequest::Enroll {
                frames_count,
                reply: reply_tx,
            })
            .await
            .map_err(|_| EngineError::ChannelClosed)?;
        reply_rx.await.map_err(|_| EngineError::ChannelClosed)?
    }

    /// Request verification: capture frames, detect, extract, compare against gallery.
    pub async fn verify(
        &self,
        gallery: Vec<FaceModel>,
        threshold: f32,
        frames_count: usize,
    ) -> Result<VerifyResult, EngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(EngineRequest::Verify {
                gallery,
                threshold,
                frames_count,
                reply: reply_tx,
            })
            .await
            .map_err(|_| EngineError::ChannelClosed)?;
        reply_rx.await.map_err(|_| EngineError::ChannelClosed)?
    }
}

/// Spawn the engine on a dedicated OS thread.
///
/// Opens the camera, loads both ONNX models, discards warmup frames,
/// then enters a request loop. Fails fast at startup if any resource
/// is unavailable.
pub fn spawn_engine(
    camera_device: &str,
    scrfd_path: &str,
    arcface_path: &str,
    warmup_frames: usize,
) -> Result<EngineHandle, EngineError> {
    // Open camera and load models synchronously (fail-fast)
    let camera = Camera::open(camera_device)?;
    tracing::info!(
        device = camera_device,
        width = camera.width,
        height = camera.height,
        fourcc = ?camera.fourcc,
        "camera opened"
    );

    let mut detector = visage_core::FaceDetector::load(scrfd_path)?;
    tracing::info!(path = scrfd_path, "SCRFD detector loaded");

    let mut recognizer = visage_core::FaceRecognizer::load(arcface_path)?;
    tracing::info!(path = arcface_path, "ArcFace recognizer loaded");

    // Discard warmup frames for camera AGC/AE stabilization
    if warmup_frames > 0 {
        tracing::info!(count = warmup_frames, "discarding warmup frames");
        for _ in 0..warmup_frames {
            let _ = camera.capture_frame();
        }
    }

    let (tx, mut rx) = mpsc::channel::<EngineRequest>(4);

    std::thread::Builder::new()
        .name("visage-engine".into())
        .spawn(move || {
            tracing::info!("engine thread started");
            while let Some(req) = rx.blocking_recv() {
                match req {
                    EngineRequest::Enroll {
                        frames_count,
                        reply,
                    } => {
                        let result = run_enroll(&camera, &mut detector, &mut recognizer, frames_count);
                        let _ = reply.send(result);
                    }
                    EngineRequest::Verify {
                        gallery,
                        threshold,
                        frames_count,
                        reply,
                    } => {
                        let result = run_verify(
                            &camera,
                            &mut detector,
                            &mut recognizer,
                            &gallery,
                            threshold,
                            frames_count,
                        );
                        let _ = reply.send(result);
                    }
                }
            }
            tracing::info!("engine thread exiting");
        })
        .expect("failed to spawn engine thread");

    Ok(EngineHandle { tx })
}

/// Capture frames, pick the best face (highest confidence), extract embedding.
fn run_enroll(
    camera: &Camera,
    detector: &mut visage_core::FaceDetector,
    recognizer: &mut visage_core::FaceRecognizer,
    frames_count: usize,
) -> Result<EnrollResult, EngineError> {
    let (frames, dark_skipped) = camera.capture_frames(frames_count)?;
    tracing::debug!(
        captured = frames.len(),
        dark_skipped,
        "enroll: captured frames"
    );

    if frames.is_empty() {
        return Err(EngineError::NoFaceDetected);
    }

    // Find the frame with the best (highest confidence) face detection
    let mut best_face = None;
    let mut best_confidence = 0.0f32;
    let mut best_frame_idx = 0;

    for (i, frame) in frames.iter().enumerate() {
        let faces = detector.detect(&frame.data, frame.width, frame.height)?;
        if let Some(face) = faces.first() {
            if face.confidence > best_confidence {
                best_confidence = face.confidence;
                best_face = Some(face.clone());
                best_frame_idx = i;
            }
        }
    }

    let face = best_face.ok_or(EngineError::NoFaceDetected)?;
    let frame = &frames[best_frame_idx];

    tracing::info!(
        confidence = face.confidence,
        frame = best_frame_idx,
        "enroll: best face selected"
    );

    let embedding = recognizer.extract(&frame.data, frame.width, frame.height, &face)?;

    Ok(EnrollResult {
        embedding,
        quality_score: best_confidence,
    })
}

/// Capture frames, detect faces, extract embeddings, compare against gallery.
/// Uses the best match across all captured frames.
fn run_verify(
    camera: &Camera,
    detector: &mut visage_core::FaceDetector,
    recognizer: &mut visage_core::FaceRecognizer,
    gallery: &[FaceModel],
    threshold: f32,
    frames_count: usize,
) -> Result<VerifyResult, EngineError> {
    let (frames, dark_skipped) = camera.capture_frames(frames_count)?;
    tracing::debug!(
        captured = frames.len(),
        dark_skipped,
        "verify: captured frames"
    );

    if frames.is_empty() {
        return Err(EngineError::NoFaceDetected);
    }

    let matcher = CosineMatcher;
    let mut best_result: Option<MatchResult> = None;
    let mut best_quality = 0.0f32;
    let mut any_face_detected = false;

    for frame in &frames {
        let faces = detector.detect(&frame.data, frame.width, frame.height)?;
        let Some(face) = faces.first() else {
            continue;
        };
        any_face_detected = true;

        let embedding = recognizer.extract(&frame.data, frame.width, frame.height, face)?;
        let result = matcher.compare(&embedding, gallery, threshold);

        let is_better = match &best_result {
            None => true,
            Some(prev) => result.similarity > prev.similarity,
        };
        if is_better {
            best_quality = face.confidence;
            best_result = Some(result);
        }
    }

    if !any_face_detected {
        return Err(EngineError::NoFaceDetected);
    }

    // If no match result at all, return a non-match
    let result = best_result.unwrap_or(MatchResult {
        matched: false,
        similarity: 0.0,
        model_id: None,
        model_label: None,
    });

    Ok(VerifyResult {
        result,
        best_quality,
    })
}
