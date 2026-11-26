import { httpFetch, cacheAvatar, resolveFacebookAvatar } from '$lib/api/social-media';
import type { ParsedProfile } from '$lib/utils/social_media_parser';

export interface CreatorMetadata {
  platform: string;
  platformName: string;
  name: string;
  handle: string;
  meta: string;
  avatar: string;
  url: string;
}

export function decodeHtmlEntities(str: string): string {
  return str
    .replace(/&#x([0-9a-fA-F]+);/g, (_, hex) => String.fromCharCode(parseInt(hex, 16)))
    .replace(/&#([0-9]+);/g, (_, dec) => String.fromCharCode(parseInt(dec, 10)))
    .replace(/&amp;/g, '&')
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>');
}

export function formatCount(countStr: string): string {
  const match = countStr.match(/([\d.,]+)\s*([KkMmBb])?/i);
  if (!match) return countStr;
  const num = parseFloat(match[1].replace(/,/g, ''));
  const unit = (match[2] || '').toUpperCase();
  if (isNaN(num)) return countStr;
  return `${num}${unit}`;
}

const memoryCache = new Map<string, { meta: CreatorMetadata; time: number }>();
const CACHE_TTL_MS = 24 * 60 * 60 * 1000;

function getCacheKey(platform: string, target: string): string {
  return `${platform.toLowerCase()}:${target.toLowerCase().trim()}`;
}

export function getCachedCreator(platform: string, target: string): CreatorMetadata | null {
  if (!platform || !target) return null;
  const key = getCacheKey(platform, target);
  const mem = memoryCache.get(key);
  if (mem && Date.now() - mem.time < CACHE_TTL_MS) {
    return mem.meta;
  }

  if (typeof localStorage !== 'undefined') {
    try {
      const raw = localStorage.getItem(`social_media_creator_${key}`);
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed && parsed.meta && Date.now() - parsed.time < CACHE_TTL_MS) {
          memoryCache.set(key, parsed);
          return parsed.meta;
        }
      }
    } catch {}
  }
  return null;
}

export function setCachedCreator(platform: string, target: string, meta: CreatorMetadata): void {
  if (!platform || !target || !meta) return;
  const key = getCacheKey(platform, target);
  const entry = { meta, time: Date.now() };
  memoryCache.set(key, entry);

  if (typeof localStorage !== 'undefined') {
    try {
      localStorage.setItem(`social_media_creator_${key}`, JSON.stringify(entry));
    } catch {}
  }
}

export async function resolveCreatorMetadata(p: ParsedProfile): Promise<CreatorMetadata | null> {
  if (!p.username && !p.cleanUrl) return null;

  const target = p.username || p.cleanUrl;
  const cached = getCachedCreator(p.platform, target);
  if (cached && cached.avatar) {
    return cached;
  }

  const platformLabels: Record<string, string> = {
    instagram: 'Instagram',
    tiktok: 'TikTok',
    facebook: 'Facebook',
    x: 'X',
  };
  const platName = platformLabels[p.platform] || p.platform;
  const handle = p.username ? `@${p.username}` : p.cleanUrl;
  let name = p.username || platName;
  let meta = `${handle} • ${platName}`;
  let avatar = '';

  if (p.platform === 'instagram' && p.username) {
    try {
      const res = await httpFetch({
        url: `https://www.instagram.com/${encodeURIComponent(p.username)}/`,
        headers: {
          'User-Agent': 'Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)',
          'Accept-Language': 'en-US,en;q=0.9',
        },
        timeout_ms: 6000,
      });

      if (res.ok && res.body) {
        const html = res.body;
        const imgMatch = html.match(/property="og:image"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:image"/);
        const titleMatch = html.match(/property="og:title"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:title"/);
        const descMatch = html.match(/property="og:description"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:description"/);

        if (imgMatch) {
          avatar = decodeHtmlEntities(imgMatch[1]);
        }

        if (titleMatch) {
          const rawTitle = decodeHtmlEntities(titleMatch[1]);
          const nameMatch = rawTitle.match(/^(.+?)\s*\(@/);
          if (nameMatch) {
            name = nameMatch[1].trim();
          }
        }

        if (descMatch) {
          const rawDesc = decodeHtmlEntities(descMatch[1]);
          const followersMatch = rawDesc.match(/([\d.,]+[KkMmBb]?)\s*Followers/i);
          const postsMatch = rawDesc.match(/([\d.,]+[KkMmBb]?)\s*Posts/i);

          const parts = [
            handle,
            'Instagram',
            followersMatch ? `${followersMatch[1]} followers` : '',
            postsMatch ? `${postsMatch[1]} posts` : '',
          ].filter(Boolean);

          meta = parts.join(' • ');
        }
      }
    } catch {
      // Keep fallbacks
    }
  } else if (p.platform === 'tiktok' && p.username) {
    try {
      const res = await httpFetch({
        url: `https://www.tiktok.com/@${encodeURIComponent(p.username)}`,
        headers: {
          'User-Agent': 'facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)',
          'Accept-Language': 'en-US,en;q=0.9',
        },
        timeout_ms: 6000,
      });

      if (res.ok && res.body) {
        const html = res.body;
        const imgMatch = html.match(/property="og:image"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:image"/);
        const titleMatch = html.match(/property="og:title"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:title"/);
        const descMatch = html.match(/property="og:description"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:description"/);

        if (imgMatch) {
          avatar = decodeHtmlEntities(imgMatch[1]);
        }

        if (titleMatch) {
          const rawTitle = decodeHtmlEntities(titleMatch[1]);
          const cleanedTitle = rawTitle.replace(/\s+on TikTok$/i, '').trim();
          if (cleanedTitle) {
            name = cleanedTitle;
          }
        }

        if (descMatch) {
          const rawDesc = decodeHtmlEntities(descMatch[1]);
          const followersMatch = rawDesc.match(/([\d.,]+[KkMmBb]?)\s*Followers/i);
          const parts = [
            handle,
            'TikTok',
            followersMatch ? `${followersMatch[1]} followers` : '',
          ].filter(Boolean);
          meta = parts.join(' • ');
        }
      }
    } catch {
      // Keep fallbacks
    }
  } else if (p.platform === 'facebook' && (p.username || p.cleanUrl)) {
    try {
      const fbTarget = p.username ? `https://www.facebook.com/${encodeURIComponent(p.username)}` : p.cleanUrl;
      const fbResolved = await resolveFacebookAvatar(fbTarget);
      if (fbResolved?.data_url) {
        avatar = fbResolved.data_url;
        if (fbResolved.name) {
          name = fbResolved.name;
        }
      }

      if (!avatar) {
        const res = await httpFetch({
          url: fbTarget,
          headers: {
            'User-Agent': 'Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)',
            'Accept-Language': 'en-US,en;q=0.9',
          },
          timeout_ms: 6000,
        });

        if (res.ok && res.body) {
          const html = res.body;
          const imgMatch = html.match(/property="og:image"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:image"/);
          const titleMatch = html.match(/property="og:title"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:title"/);
          const descMatch = html.match(/property="og:description"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:description"/);

          if (imgMatch) {
            const rawImg = decodeHtmlEntities(imgMatch[1]);
            if (!rawImg.includes('lookaside.fbsbx.com')) {
              avatar = rawImg;
            }
          }

          if (titleMatch && !name) {
            const rawTitle = decodeHtmlEntities(titleMatch[1]);
            const cleanedTitle = rawTitle.replace(/\s*\|\s*Facebook$/i, '').trim();
            if (cleanedTitle) {
              name = cleanedTitle;
            }
          }

          if (descMatch) {
            const rawDesc = decodeHtmlEntities(descMatch[1]);
            const followersMatch = rawDesc.match(/([\d.,]+[KkMmBb]?)\s*followers/i);
            const parts = [
              handle,
              'Facebook',
              followersMatch ? `${followersMatch[1]} followers` : '',
            ].filter(Boolean);
            meta = parts.join(' • ');
          }
        }
      }
    } catch {
      // Keep fallbacks
    }
  } else if (p.platform === 'x' && p.username) {
    try {
      const res = await httpFetch({
        url: `https://api.vxtwitter.com/${encodeURIComponent(p.username)}`,
        headers: {
          'User-Agent': 'TelegramBot (like TwitterBot)',
          'Accept': 'application/json',
        },
        timeout_ms: 5000,
      });

      if (res.ok && res.body && !res.body.includes('<!DOCTYPE html>')) {
        const data = JSON.parse(res.body);
        if (data.name) name = data.name;
        if (data.profile_image_url) avatar = data.profile_image_url;
        const followers = data.followers_count ? `${data.followers_count.toLocaleString()} followers` : '';
        const parts = [handle, 'X', followers].filter(Boolean);
        meta = parts.join(' • ');
      }

      if (!avatar) {
        const fxRes = await httpFetch({
          url: `https://api.fxtwitter.com/${encodeURIComponent(p.username)}`,
          headers: {
            'User-Agent': 'Mozilla/5.0 (compatible; Googlebot/2.1)',
            'Accept': 'application/json',
          },
          timeout_ms: 5000,
        });
        if (fxRes.ok && fxRes.body && !fxRes.body.includes('<!DOCTYPE html>')) {
          const fxData = JSON.parse(fxRes.body);
          if (fxData.user) {
            if (fxData.user.name) name = fxData.user.name;
            if (fxData.user.avatar_url) avatar = fxData.user.avatar_url;
            const followers = fxData.user.followers ? `${fxData.user.followers.toLocaleString()} followers` : '';
            const parts = [handle, 'X', followers].filter(Boolean);
            meta = parts.join(' • ');
          }
        }
      }

      if (!avatar) {
        const xRes = await httpFetch({
          url: `https://x.com/${encodeURIComponent(p.username)}`,
          headers: {
            'User-Agent': 'Twitterbot/1.0',
            'Accept-Language': 'en-US,en;q=0.9',
          },
          timeout_ms: 5000,
        });
        if (xRes.ok && xRes.body) {
          const html = xRes.body;
          const imgMatch = html.match(/property="og:image"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:image"/);
          const titleMatch = html.match(/property="og:title"\s+content="([^"]+)"/) || html.match(/content="([^"]+)"\s+property="og:title"/);

          if (imgMatch) avatar = decodeHtmlEntities(imgMatch[1]);
          if (titleMatch) {
            const rawTitle = decodeHtmlEntities(titleMatch[1]);
            name = rawTitle.replace(/\s*\(@.+?\)\s*on\s*X$/i, '').trim() || name;
          }
          const parts = [handle, 'X'].filter(Boolean);
          meta = parts.join(' • ');
        }
      }

      if (avatar && avatar.includes('_normal.')) {
        avatar = avatar.replace('_normal.', '_bigger.');
      }
    } catch {
      // Keep fallbacks
    }
  }

  if (avatar) {
    avatar = await cacheAvatar(avatar);
  }

  const resolved: CreatorMetadata = {
    platform: p.platform,
    platformName: platName,
    name,
    handle,
    meta,
    avatar,
    url: p.cleanUrl,
  };

  if (avatar) {
    setCachedCreator(p.platform, target, resolved);
  }

  return resolved;
}
