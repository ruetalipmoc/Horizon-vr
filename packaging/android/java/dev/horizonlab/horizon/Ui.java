package dev.horizonlab.horizon;

import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.util.Locale;

/**
 * Shared look of Horizon VR's setup and profile screens, in the spirit of a block game's
 * menus: a pixel-block background, square panels and buttons with a raised bevel, a green
 * play button and text with a hard drop shadow. Everything is drawn here: no game art.
 */
final class Ui {
    static final int BG = Color.rgb(24, 22, 20);
    static final int CARD = Color.rgb(48, 48, 52);
    static final int EDGE = Color.rgb(16, 16, 18);
    static final int GREEN = Color.rgb(74, 140, 46);
    static final int GREEN_DARK = Color.rgb(44, 92, 26);
    static final int STONE = Color.rgb(112, 112, 116);
    static final int TEXT = Color.rgb(244, 244, 240);
    static final int MUTED = Color.rgb(176, 176, 170);
    static final int WARN = Color.rgb(252, 214, 84);
    static final int SHADOW = Color.rgb(36, 36, 36);

    private Ui() {}

    static boolean ru() {
        return "ru".equals(Locale.getDefault().getLanguage());
    }

    static String t(String ru, String en) {
        return ru() ? ru : en;
    }

    static int dp(Context c, float v) {
        return (int) (v * c.getResources().getDisplayMetrics().density + 0.5f);
    }

    /** A square panel with a dark outline and a raised bevel (the radius is ignored). */
    static android.graphics.drawable.Drawable rounded(Context c, int fill, int stroke, float radius) {
        return new Bevel(dp(c, 2), fill, stroke != 0 ? EDGE : 0, false);
    }

    static int shade(int color, float by) {
        return Color.rgb(
                Math.max(0, Math.min(255, (int) (Color.red(color) * by))),
                Math.max(0, Math.min(255, (int) (Color.green(color) * by))),
                Math.max(0, Math.min(255, (int) (Color.blue(color) * by))));
    }

    /** Fill, a lighter top and left edge, a darker bottom and right one, an outline. */
    static final class Bevel extends android.graphics.drawable.Drawable {
        private final Paint paint = new Paint();
        private final int px, fill, outline;
        private final boolean sunken;

        Bevel(int px, int fill, int outline, boolean sunken) {
            this.px = Math.max(1, px);
            this.fill = fill;
            this.outline = outline;
            this.sunken = sunken;
        }

        @Override
        public void draw(Canvas canvas) {
            android.graphics.Rect b = getBounds();
            int l = b.left, t = b.top, r = b.right, bo = b.bottom;
            if (outline != 0) {
                paint.setColor(outline);
                canvas.drawRect(l, t, r, bo, paint);
                l += px; t += px; r -= px; bo -= px;
            }
            paint.setColor(shade(fill, sunken ? 0.6f : 1.35f));
            canvas.drawRect(l, t, r, bo, paint);
            paint.setColor(shade(fill, sunken ? 1.35f : 0.6f));
            canvas.drawRect(l + px, t + px, r, bo, paint);
            paint.setColor(fill);
            canvas.drawRect(l + px, t + px, r - px, bo - px, paint);
        }

        @Override public void setAlpha(int alpha) {}
        @Override public void setColorFilter(android.graphics.ColorFilter f) {}
        @Override public int getOpacity() { return android.graphics.PixelFormat.OPAQUE; }
    }

    /** A pixel-block background: a 16x16 tile of earthy shades, scaled up without smoothing. */
    static android.graphics.drawable.Drawable blocks(Context c) {
        int[] shades = {
            Color.rgb(58, 44, 34), Color.rgb(66, 50, 38), Color.rgb(50, 38, 30),
            Color.rgb(74, 58, 44), Color.rgb(44, 34, 26), Color.rgb(62, 48, 36)
        };
        java.util.Random random = new java.util.Random(7);
        android.graphics.Bitmap tile = android.graphics.Bitmap.createBitmap(16, 16, android.graphics.Bitmap.Config.ARGB_8888);
        for (int y = 0; y < 16; y++) {
            for (int x = 0; x < 16; x++) {
                tile.setPixel(x, y, shade(shades[random.nextInt(shades.length)], 0.55f));
            }
        }
        int size = dp(c, 64);
        android.graphics.Bitmap big = android.graphics.Bitmap.createScaledBitmap(tile, size, size, false);
        android.graphics.drawable.BitmapDrawable d = new android.graphics.drawable.BitmapDrawable(c.getResources(), big);
        d.setTileModeXY(android.graphics.Shader.TileMode.REPEAT, android.graphics.Shader.TileMode.REPEAT);
        d.setFilterBitmap(false);
        return d;
    }

    /** A full-screen background with a centered card; returns the card to fill. */
    static LinearLayout card(android.app.Activity a) {
        FrameLayout root = new FrameLayout(a);
        root.setBackground(blocks(a));
        android.widget.ScrollView scroll = new android.widget.ScrollView(a);
        scroll.setFillViewport(true);
        FrameLayout center = new FrameLayout(a);
        LinearLayout card = new LinearLayout(a);
        card.setOrientation(LinearLayout.VERTICAL);
        int p = dp(a, 22);
        card.setPadding(p, p, p, p);
        card.setBackground(rounded(a, CARD, EDGE, 0));
        FrameLayout.LayoutParams lp = new FrameLayout.LayoutParams(
                Math.min(dp(a, 520), a.getResources().getDisplayMetrics().widthPixels - dp(a, 32)),
                FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.CENTER);
        lp.setMargins(0, dp(a, 16), 0, dp(a, 16));
        center.addView(card, lp);
        scroll.addView(center);
        root.addView(scroll);
        a.setContentView(root);
        return card;
    }

    static TextView text(Context c, String s, float size, int color, boolean bold) {
        TextView t = new TextView(c);
        t.setText(s);
        t.setTextSize(size);
        t.setTextColor(color);
        t.setTypeface(bold ? Typeface.create("sans-serif-medium", Typeface.BOLD) : Typeface.create("sans-serif", Typeface.NORMAL));
        t.setLineSpacing(0, 1.15f);
        // The menus' hard drop shadow.
        t.setShadowLayer(0.01f, dp(c, 1.5f), dp(c, 1.5f), SHADOW);
        t.setPadding(0, dp(c, 4), 0, dp(c, 4));
        return t;
    }

    static Button button(Context c, String s, boolean primary) {
        Button b = new Button(c);
        b.setText(s);
        b.setAllCaps(false);
        b.setTextSize(primary ? 17 : 15);
        b.setTextColor(primary ? Color.WHITE : TEXT);
        b.setTypeface(Typeface.create("sans-serif-medium", Typeface.BOLD));
        int fill = primary ? GREEN : STONE;
        android.graphics.drawable.StateListDrawable states = new android.graphics.drawable.StateListDrawable();
        states.addState(new int[] {-android.R.attr.state_enabled}, new Bevel(dp(c, 2), shade(fill, 0.55f), EDGE, false));
        states.addState(new int[] {android.R.attr.state_pressed}, new Bevel(dp(c, 2), shade(fill, 0.8f), EDGE, true));
        states.addState(new int[] {android.R.attr.state_focused}, new Bevel(dp(c, 2), shade(fill, 1.15f), Color.WHITE, false));
        states.addState(new int[] {android.R.attr.state_hovered}, new Bevel(dp(c, 2), shade(fill, 1.15f), Color.WHITE, false));
        states.addState(new int[] {}, new Bevel(dp(c, 2), fill, EDGE, false));
        b.setBackground(states);
        b.setTextColor(Color.WHITE);
        b.setShadowLayer(0.01f, dp(c, 1.5f), dp(c, 1.5f), SHADOW);
        b.setMinHeight(dp(c, primary ? 54 : 44));
        b.setStateListAnimator(null);
        return b;
    }

    static LinearLayout.LayoutParams gap(Context c, int top) {
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT);
        lp.topMargin = dp(c, top);
        return lp;
    }

    /** A section header: a thin divider and a small green caption (empty title: divider only). */
    static void section(LinearLayout card, String title) {
        Context c = card.getContext();
        View line = new View(c);
        line.setBackgroundColor(EDGE);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, Math.max(1, dp(c, 1)));
        lp.topMargin = dp(c, 18);
        lp.bottomMargin = dp(c, 8);
        card.addView(line, lp);
        if (title.isEmpty()) return;
        TextView t = text(c, title.toUpperCase(Locale.getDefault()), 12, GREEN, true);
        t.setLetterSpacing(0.08f);
        card.addView(t);
    }

    /** A rounded progress bar, 0..1000. */
    static final class Bar extends View {
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        private final RectF r = new RectF();
        private int value;

        Bar(Context c) {
            super(c);
            setMinimumHeight(dp(c, 14));
        }

        void set(int permille) {
            value = Math.max(0, Math.min(1000, permille));
            invalidate();
        }

        @Override
        protected void onMeasure(int w, int h) {
            setMeasuredDimension(MeasureSpec.getSize(w), dp(getContext(), 14));
        }

        @Override
        protected void onDraw(Canvas canvas) {
            int px = dp(getContext(), 2);
            paint.setColor(EDGE);
            canvas.drawRect(0, 0, getWidth(), getHeight(), paint);
            paint.setColor(Color.rgb(28, 28, 30));
            canvas.drawRect(px, px, getWidth() - px, getHeight() - px, paint);
            if (value > 0) {
                float right = px + (getWidth() - 2 * px) * value / 1000f;
                paint.setColor(GREEN);
                canvas.drawRect(px, px, right, getHeight() - px, paint);
                paint.setColor(shade(GREEN, 1.35f));
                canvas.drawRect(px, px, right, px * 2, paint);
            }
        }
    }
}
