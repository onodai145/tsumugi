/**
 * jsdom テスト環境の補完。
 *
 * jsdom は DOM Core の基本実装のみを提供し、メディア関連や Observer
 * など一部のブラウザ API が不足している。このファイルは Vitest の
 * setupFiles で読み込まれ、テスト実行環境を補完する。
 */

// jsdom は scrollIntoView を実装していないため、no-op スタブを用意
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}

// Vidstack/media-player のズーム・レスポンシブ機能に必須
if (!window.matchMedia) {
  window.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => true,
  });
}

// Vidstack のレイアウト計算に必須
if (!window.ResizeObserver) {
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  window.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;
}

// Vidstack のメディアローダーに必須
if (!window.IntersectionObserver) {
  class IntersectionObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
    takeRecords() {
      return [];
    }
  }
  window.IntersectionObserver = IntersectionObserverStub as unknown as typeof IntersectionObserver;
}

// media-captions が VTTCue を拡張しようとするが、jsdom では undefined
// VTTCue は TextTrackCue を拡張し、WebVTT 字幕のキューを表現する API
// 詳細: https://html.spec.whatwg.org/multipage/media.html#the-vttcue-interface
if (!window.VTTCue) {
  // TextTrackCue のスタブ（VTTCue の基底）
  class TextTrackCueStub extends EventTarget {
    startTime = 0;
    endTime = 0;
    pauseOnExit = false;

    constructor(startTime: number, endTime: number) {
      super();
      this.startTime = startTime;
      this.endTime = endTime;
    }
  }

  // VTTCue スタブ
  class VTTCueStub extends TextTrackCueStub {
    region: unknown = null;
    vertical = "";
    snapToLines = true;
    line: string | number = "auto";
    lineAlign = "start";
    position: string | number = "auto";
    positionAlign = "auto";
    size = 100;
    textAlign = "center";
    text = "";

    constructor(startTime: number, endTime: number, text: string) {
      super(startTime, endTime);
      this.text = text;
    }
  }

  window.VTTCue = VTTCueStub as unknown as typeof VTTCue;
}
