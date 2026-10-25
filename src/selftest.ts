/**
 * Kitty graphics のデータ転送セルフテスト。
 *
 * 「クエリには OK を返すが、実際の画像データは描画されない」端末・経路
 * （例: Windows の WezTerm + ssh.exe (ConPTY)、wezterm#8070）を検出するため、
 * 4096 文字を超える（= 複数チャンクの）PNG を送って応答を確認する。
 */

export const KITTY_TEST_ID = 987654;

/** 32x32 のノイズ PNG（base64 で 2 チャンクになるサイズ）。 */
const TEST_PNG_BASE64 = [
  "iVBORw0KGgoAAAANSUhEUgAAACAAAAAgCAYAAABzenr0AAAACXBIWXMAAAPoAAAD6AG1e1JrAAAQK0lEQVRYCQEgEN/vAExQlf/i",
  "+4v/Vzvk/yYfrf8Pxlb/BzWW/5pY4//sU1X/AUoi/3t6M/8Av17/8zzO/540JP9+zNz/uwOR/2FIO//rUlz/x180/7s5gf/kROn/",
  "h8l2/7g1Df/Hjv3/lk4L/+y8Mf/Nwkb/brfg/6KV1v8xD7L/xSHE/6fof/+4h6z/AOIka//LahT/HDsD/1mZdP/nqG//GDhk/zyy",
  "w/9vaA7/vq2F/75E4P/rulL/DW45/7z2Rf9TPyn/eRsb/5UK2f9vdo//DYlt/6olYP/64Z//tH5E/1e4AP+2N6P/yG/N/6hJL/9u",
  "xG7/10a1/5pmC//bnCP/Dh6U/9e0mf/+4M7/AGW6vf/tQKz/nY5G/7CJTP9RKUD/gmJh//Vhnf/0Ygb/akFl//A9DP8SIeX/yWG/",
  "/+U7Jv9CjAj/V6ab/9v2tf/7VuD/75Ci/3zVfv9aHXT/Yswy/36ppP8/PbL/7F4Y/22kUv8hDFD/jwkU//h4DP/+b1L/K1Hd/7Jv",
  "3f/pI0j/AAJ10//Rr3z/Tb/C/wNdLv/W6jb/UxfZ/31iNv8gdn7/oiG5/9P9Yv9v7L3/ZmU8/3ZgTP9xINj//Ekc/3jdNv8k7vX/",
  "4fDS/3EITv+I3p//wcoX/8kLYP9tCbT/JAV7/9z8Nv/24K//uMqR/3GIov8uJur/+g5p/1BtXP8teP3/AM78+P9vKML/FNE5/3SN",
  "mv9mLaP/pmP2/zgY4f+p5sL/ynHt/9OiUP9TVY3/gMJX/4vN2P9jCYn/I1Jj/wKqe/+a3Rb/AaDH/4ELXP+ylRD/Bz6//ytwtf8F",
  "qqv/oqZY/2EoJ//X32n/h75M/1IKuv8jqJb/0eLW/3W6sf8SBNn/AGVvG//xWn7/iM4Z/4HLoP+DdjX/yAr7//M3Rv+N7pT/fHZD",
  "/5xa7P/I9ff/Q2Ia/0Md6/+1I2P/3hl//+Xgtf80BAn/o0m9/8Nt3f/vg97/uXHj/3k/7v/UfDH/wSIB/+fqBf+Q+Bj/xFHI/yl+",
  "uv9aHlf/aFsi/0XrE/+fODz/AGFSRP84qMH/pWy+/5Kn4f/gd4z/HC0b/3AKev+4Zaz/AMTO/0qNrP8JWv//fPdI/9XyYP/3mIP/",
  "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",
  "/64zqf/olqf/+qR3/yky1P9FVE//Tdt9/61hRP846tn/9+zu/zBJy/+I7UH/8eqM/8CcpP/4Lfv/YfPM/9DFj//NxZ7/l+fE/37F",
  "8f8zNir/ALvF9v+P9Kv/vD9B/4adQv/OQXn/p/t1/8tlEP/kmRL/QEJ+/1gKtv9GWcj/k7U3/6X2Jv/aRQT/GsH5/8l6WP+4lgj/",
  "jyHc/5rzfv+gcGz/JYVF/1YTa//fa8D/Rmrx/4VgPP8DYFz/SG7X/4roe/9dzmj/Wz4d/82UQP9GT5j/ADuGWf/6DJz/FxPP/+sk",
  "sP+J0cn/7gpS/xuNnv/wWPj/SrAY//0dK/+YW5X//kdC/x465v9kbeT/cIPe/9W9Uv8hHcD/ihHi/yQKGv8awhj/cOeN/6IMXf+Y",
  "733/dk1t/6EAof+4nL//jUhk/8fv7P/2VRz/fLtX/zeYav9sjcH/AGdLXf9P60T/b3br/6smXP+sx6X/hOvQ/yMQh/8CSaz/8gIA",
  "/x9wlP8vgEb/35Zj/xtNbf+r2Bb/mT0G/ze2HP9i6y7/hk65/8gpZv8fOiL/j6/T/zfJ7v/BWRj/jXVb/5Y20f/cFAL/PoYb/yaV",
  "MP9xaNX/WStk/6c70f/m/Oj/ALT0/P+Hq5b/3i27//TeaP8nHAf/8YDD/wynJ/85u8f/wWgF/3UyT/9hQeP/M1jP//N03P/g8UX/",
  "XJ8Z/xJ/tv/2mW//gqex/x0huf9xrOb/a4iw/5ByEf/s6Ov/e2DU/zmLBP9Sw8z/sgma/xvRJv9m5VT/JaNo/5iZM//qKff/YYX7",
  "IIdomEQAAAAASUVORK5CYII=",
].join("");

export function kittySelfTestSequence(): string {
  const chunks: string[] = [];
  for (let i = 0; i < TEST_PNG_BASE64.length; i += 4096) {
    chunks.push(TEST_PNG_BASE64.slice(i, i + 4096));
  }

  const parts: string[] = [];
  chunks.forEach((chunk, index) => {
    const more = index === chunks.length - 1 ? 0 : 1;
    const control =
      index === 0
        ? `a=t,f=100,I=${KITTY_TEST_ID},q=0,m=${more}`
        : `m=${more}`;
    parts.push(`\x1b_G${control};${chunk}\x1b\\`);
  });
  // テスト用画像を後始末する（画像番号 I= で削除。i= は使わない）
  parts.push(`\x1b_Ga=d,d=n,I=${KITTY_TEST_ID},q=2\x1b\\`);
  return parts.join("");
}

/**
 * セルフテストの応答を判定する。
 *
 * WezTerm は `i=` と `I=` の同時指定を不正として扱うため、テストでは
 * 画像番号 (`I=`) のみを使う。応答は `_GI=<no>,i=<id>;OK` または `_GI=<no>;OK`。
 */
export function parseKittySelfTest(raw: string): boolean {
  // 応答は端末ごとに異なる（`_GI=<no>,i=<id>;OK` / `_Gi=<id>;OK` / `_G;OK`）。
  // クエリ（`i=31`）の応答だけを除外して、それ以外の OK を自己テスト成功とみなす。
  const okResponses = [...raw.matchAll(/_G([^;]*);OK/g)];
  return okResponses.some((match) => {
    const control = match[1] ?? "";
    return !/(?:^|,)i=31(?:,|$)/.test(control);
  });
}
