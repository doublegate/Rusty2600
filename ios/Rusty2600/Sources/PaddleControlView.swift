import SwiftUI

/// A virtual analog paddle — genuinely new UX work `android/`'s v1.11.0
/// build didn't need (RustyNES's touch overlay is d-pad-only; the 2600
/// paddle is the first analog input either mobile host has to solve).
///
/// Modeled as a touch-drag rotary dial (like the real Atari paddle
/// controller's physical knob) rather than a device-tilt control:
/// tilt would need Core Motion + a `CMMotionManager` permission and drifts
/// without a rezero gesture, while a drag dial maps directly and
/// deterministically to `MobilePaddle.position`, matching how a real
/// paddle's potentiometer has a fixed, bounded rotation range rather than
/// spinning freely.
///
/// A real paddle's pot sweeps roughly 300 degrees, not a full continuous
/// turn — modeled here as a clamped -150...+150 degree arc (0 straight up),
/// 0 at the counter-clockwise limit and 255 at the clockwise limit, matching
/// `MobilePaddle`'s own doc comment ("0 fully clockwise ..= 255 fully
/// counter-clockwise" is the TIA-register convention; this view's `position`
/// output already matches that convention directly, no inversion needed by
/// the caller).
struct PaddleControlView: View {
    /// `0...255`, matching `MobilePaddle.position` directly.
    @Binding var position: UInt8
    @State private var fire = false
    var onFireChanged: (Bool) -> Void = { _ in }

    private let minDegrees: Double = -150
    private let maxDegrees: Double = 150

    var body: some View {
        VStack(spacing: 12) {
            GeometryReader { geo in
                let center = CGPoint(x: geo.size.width / 2, y: geo.size.height / 2)
                ZStack {
                    Circle()
                        .strokeBorder(Color.gray, lineWidth: 3)
                    knob(center: center, radius: min(geo.size.width, geo.size.height) / 2 - 12)
                }
                .contentShape(Circle())
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            updatePosition(from: value.location, center: center)
                        }
                )
            }
            .frame(width: 140, height: 140)

            Button {
                fire.toggle()
                onFireChanged(fire)
            } label: {
                Text("FIRE")
                    .frame(width: 80, height: 40)
                    .background(fire ? Color.red : Color.gray.opacity(0.4))
                    .foregroundColor(.white)
                    .clipShape(Capsule())
            }
        }
    }

    private func knob(center: CGPoint, radius: CGFloat) -> some View {
        let degrees = minDegrees + (Double(position) / 255.0) * (maxDegrees - minDegrees)
        let radians = (degrees - 90) * .pi / 180
        let x = center.x + radius * cos(radians)
        let y = center.y + radius * sin(radians)
        return Circle()
            .fill(Color.accentColor)
            .frame(width: 20, height: 20)
            .position(x: x, y: y)
    }

    private func updatePosition(from location: CGPoint, center: CGPoint) {
        let dx = location.x - center.x
        let dy = location.y - center.y
        guard dx != 0 || dy != 0 else { return }

        // `atan2` returns -180...180 measured from the positive x-axis;
        // rotate so 0 degrees is straight up (matching the knob's drawing
        // convention above) and clamp to the paddle's physical sweep range.
        var degrees = atan2(dy, dx) * 180 / .pi + 90
        if degrees > 180 { degrees -= 360 }
        let clamped = min(max(degrees, minDegrees), maxDegrees)
        let fraction = (clamped - minDegrees) / (maxDegrees - minDegrees)
        position = UInt8(max(0, min(255, (fraction * 255).rounded())))
    }
}
