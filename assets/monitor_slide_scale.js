// Fits a slide into a box that is not the size of a screen.
//
// A slide's type is set in *points*, which are absolute: put the renderer into
// a smaller box and the words come out at their full size and overflow it. So
// the slide is laid out at the presentation's own size and the whole thing is
// shrunk with a transform — the same trick every other preview in Cantara
// uses.
//
// The factor cannot be worked out in Rust: the box is a share of a window
// whose size is the operator's business, and on the network a share of a phone
// nobody here can measure. It was CSS for a while —
// `scale(min(calc(100cqw / var(--slide-width)), …))` — which is exactly right
// and is dropped by any engine without container-query *division*. Cantara's
// own web view is one of them, and a dropped declaration means no scaling at
// all: the slide overflowed its box and the next one beside it was drawn at
// full size.
//
// So it is measured. This runs in two places and is one file for that reason:
// the window evaluates it when a slide frame mounts, and the page served to
// the network runs it after it has been given new markup. The arithmetic is
// the same arithmetic, and a second copy of it would be a second thing to get
// wrong.
(function () {
  function fit(frame) {
    var stage = frame.querySelector('.monitor-slide-stage');
    if (!stage) return;

    var style = getComputedStyle(frame);
    // The presentation's own size, as the markup states it. Falling back to
    // 16:9 at 1920 rather than to nothing: a frame with no size named is
    // better drawn at a guess than left overflowing.
    var slideWidth = parseFloat(style.getPropertyValue('--slide-width')) || 1920;
    var slideHeight = parseFloat(style.getPropertyValue('--slide-height')) || 1080;

    var width = frame.clientWidth;
    var height = frame.clientHeight;
    // Not laid out yet. Leaving it alone is right: this runs again on the next
    // resize, and a scale computed from nothing would be zero.
    if (!width || !height) return;

    // The smaller of the two ratios is the one that fits in both directions,
    // which is what keeps the proportions.
    var scale = Math.min(width / slideWidth, height / slideHeight);

    stage.style.transformOrigin = 'top left';
    stage.style.transform = 'scale(' + scale + ')';
    // Centred in whichever direction has room left over, so a 4:3 slide in a
    // 16:9 box does not sit against the left edge.
    stage.style.left = Math.max(0, (width - slideWidth * scale) / 2) + 'px';
    stage.style.top = Math.max(0, (height - slideHeight * scale) / 2) + 'px';
  }

  function fitAll() {
    document.querySelectorAll('.monitor-slide-frame').forEach(fit);
  }

  // Once now, and again whenever the window changes shape. Registered once
  // however many times this is evaluated: the window evaluates it per slide
  // frame, and a listener per frame per slide change would pile up for the
  // whole of a service.
  if (!window.__cantaraSlideScale) {
    window.__cantaraSlideScale = true;
    window.addEventListener('resize', fitAll);
  }

  // The frame may not be laid out in the same tick it was mounted in, so this
  // is asked for again on the next frame. Both, rather than only the later
  // one, so that a slide that *is* ready does not flash at full size first.
  fitAll();
  requestAnimationFrame(fitAll);
})();
