package dev.horizonlab.horizon;

/**
 * The game on a flat panel (2D), as on a phone: declared without the Quest's VR category
 * so the headset shows it in a window, in a process of its own (":flat") running the
 * original flat game (libhorizon2d.so). It shares the app's folder, so worlds, options
 * and the skin are the same as in VR.
 */
public class HorizonFlatActivity extends HorizonActivity {
}
