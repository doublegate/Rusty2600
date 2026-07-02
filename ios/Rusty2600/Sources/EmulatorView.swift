import Metal
import MetalKit
import SwiftUI

/// Matches `rusty2600_mobile::ATARI_W`.
let frameWidth = 160
/// Matches `rusty2600_mobile::ATARI_H`.
let frameHeight = 192

/// Renders the 160x192 RGBA8 framebuffer `MobileEmulator.runFrame` produces,
/// via a single textured full-screen quad — the Metal counterpart to
/// Android's `EmulatorView.copyPixelsFromBuffer`. A full custom render
/// pipeline (multiple passes, shaders beyond a plain texture sample) isn't
/// warranted here: this is a one-texture blit, not a shader-stack renderer
/// (that's `v1.4.0`'s `rusty2600-gfx-shaders`, a desktop-only feature this
/// mobile bridge doesn't consume).
struct EmulatorView: UIViewRepresentable {
    /// The latest frame's RGBA8 bytes (`frameWidth * frameHeight * 4`).
    @Binding var rgba: [UInt8]

    func makeCoordinator() -> Renderer {
        Renderer()
    }

    func makeUIView(context: Context) -> MTKView {
        let view = MTKView()
        view.device = MTLCreateSystemDefaultDevice()
        view.delegate = context.coordinator
        view.enableSetNeedsDisplay = false
        view.isPaused = false
        view.preferredFramesPerSecond = 60
        view.colorPixelFormat = .bgra8Unorm
        context.coordinator.configure(device: view.device)
        return view
    }

    func updateUIView(_ uiView: MTKView, context: Context) {
        context.coordinator.updateFrame(rgba)
    }

    /// Nearest-neighbor upload of the RGBA8 buffer into a texture, then a
    /// full-screen-triangle draw sampling it — no filtering, matching the
    /// native/wasm/Android frontends' pixelated look rather than a smoothed
    /// blow-up.
    final class Renderer: NSObject, MTKViewDelegate {
        private var device: MTLDevice?
        private var commandQueue: MTLCommandQueue?
        private var pipelineState: MTLRenderPipelineState?
        private var texture: MTLTexture?
        private var pendingRGBA: [UInt8]?
        private let lock = NSLock()

        func configure(device: MTLDevice?) {
            guard let device else { return }
            self.device = device
            commandQueue = device.makeCommandQueue()

            let textureDescriptor = MTLTextureDescriptor.texture2DDescriptor(
                pixelFormat: .rgba8Unorm,
                width: frameWidth,
                height: frameHeight,
                mipmapped: false
            )
            textureDescriptor.usage = [.shaderRead]
            texture = device.makeTexture(descriptor: textureDescriptor)

            guard let library = try? device.makeLibrary(source: Self.shaderSource, options: nil) else {
                return
            }
            let descriptor = MTLRenderPipelineDescriptor()
            descriptor.vertexFunction = library.makeFunction(name: "fullscreenQuadVertex")
            descriptor.fragmentFunction = library.makeFunction(name: "sampleTextureFragment")
            descriptor.colorAttachments[0].pixelFormat = .bgra8Unorm
            pipelineState = try? device.makeRenderPipelineState(descriptor: descriptor)
        }

        /// Called from the emulation loop (a background thread/task); the
        /// actual texture upload happens on `draw(in:)` (the MTKView's own
        /// thread), guarded by `lock` since the two run concurrently.
        func updateFrame(_ rgba: [UInt8]) {
            lock.lock()
            pendingRGBA = rgba
            lock.unlock()
        }

        func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

        func draw(in view: MTKView) {
            lock.lock()
            let rgba = pendingRGBA
            pendingRGBA = nil
            lock.unlock()

            if let rgba, rgba.count == frameWidth * frameHeight * 4, let texture {
                rgba.withUnsafeBytes { buffer in
                    texture.replace(
                        region: MTLRegionMake2D(0, 0, frameWidth, frameHeight),
                        mipmapLevel: 0,
                        withBytes: buffer.baseAddress!,
                        bytesPerRow: frameWidth * 4
                    )
                }
            }

            guard
                let pipelineState,
                let texture,
                let commandQueue,
                let descriptor = view.currentRenderPassDescriptor,
                let drawable = view.currentDrawable,
                let commandBuffer = commandQueue.makeCommandBuffer(),
                let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: descriptor)
            else {
                return
            }

            encoder.setRenderPipelineState(pipelineState)
            encoder.setFragmentTexture(texture, index: 0)
            encoder.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: 3)
            encoder.endEncoding()
            commandBuffer.present(drawable)
            commandBuffer.commit()
        }

        /// A full-screen triangle (the standard 3-vertex trick — cheaper
        /// than a 4-vertex/6-index quad and covers the same viewport) with a
        /// plain nearest-sampled texture fetch; no post-processing.
        private static let shaderSource = """
        #include <metal_stdlib>
        using namespace metal;

        struct VertexOut {
            float4 position [[position]];
            float2 uv;
        };

        vertex VertexOut fullscreenQuadVertex(uint vertexID [[vertex_id]]) {
            float2 positions[3] = {
                float2(-1.0, -1.0),
                float2(-1.0,  3.0),
                float2( 3.0, -1.0)
            };
            float2 uvs[3] = {
                float2(0.0, 1.0),
                float2(0.0, -1.0),
                float2(2.0, 1.0)
            };
            VertexOut out;
            out.position = float4(positions[vertexID], 0.0, 1.0);
            out.uv = uvs[vertexID];
            return out;
        }

        fragment float4 sampleTextureFragment(VertexOut in [[stage_in]],
                                               texture2d<float> tex [[texture(0)]]) {
            constexpr sampler nearestSampler(mag_filter::nearest, min_filter::nearest);
            return tex.sample(nearestSampler, in.uv);
        }
        """
    }
}
