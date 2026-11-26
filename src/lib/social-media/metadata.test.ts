import { describe, it, expect, vi } from 'vitest';
import { decodeHtmlEntities, resolveCreatorMetadata } from './metadata';
import * as socialMediaApi from '$lib/api/social-media';

vi.mock('$lib/api/social-media', () => ({
  httpFetch: vi.fn(),
  cacheAvatar: vi.fn(async (url: string) => url),
  resolveFacebookAvatar: vi.fn(async () => null),
}));

describe('social-media metadata extraction', () => {
  it('decodes HTML entities properly', () => {
    expect(decodeHtmlEntities('&#xbaa8;&#xbaa8; (MOMO)')).toBe('모모 (MOMO)');
    expect(decodeHtmlEntities('A &amp; B &#064;test')).toBe('A & B @test');
  });

  it('extracts Instagram metadata from OpenGraph tags', async () => {
    const mockHtml = `
      <html>
        <head>
          <meta property="og:image" content="https://scontent.cdninstagram.com/avatar.jpg" />
          <meta property="og:title" content="&#xbaa8;&#xbaa8; (MOMO) (&#064;momo) &bull; Instagram photos and videos" />
          <meta property="og:description" content="16M Followers, 0 Following, 463 Posts - See Instagram photos and videos" />
        </head>
      </html>
    `;

    vi.mocked(socialMediaApi.httpFetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      headers: {},
      body: mockHtml,
    });

    const meta = await resolveCreatorMetadata({
      platform: 'instagram',
      username: 'momo',
      cleanUrl: 'https://www.instagram.com/momo/',
      rawInput: 'https://www.instagram.com/momo/',
      isSpecificMedia: false,
    });

    expect(meta).not.toBeNull();
    expect(meta?.name).toBe('모모 (MOMO)');
    expect(meta?.avatar).toBe('https://scontent.cdninstagram.com/avatar.jpg');
    expect(meta?.meta).toContain('16M followers');
    expect(meta?.meta).toContain('463 posts');
  });

  it('extracts X/Twitter metadata from vxtwitter endpoint', async () => {
    vi.mocked(socialMediaApi.httpFetch).mockResolvedValueOnce({
      ok: true,
      status: 200,
      headers: {},
      body: JSON.stringify({
        name: 'OpenAI',
        followers_count: 5000000,
        profile_image_url: 'https://pbs.twimg.com/avatar.jpg',
      }),
    });

    const meta = await resolveCreatorMetadata({
      platform: 'x',
      username: 'OpenAI',
      cleanUrl: 'https://x.com/OpenAI',
      rawInput: 'OpenAI',
      isSpecificMedia: false,
    });

    expect(meta).not.toBeNull();
    expect(meta?.name).toBe('OpenAI');
    expect(meta?.avatar).toBe('https://pbs.twimg.com/avatar.jpg');
    expect(meta?.meta).toContain('5,000,000 followers');
  });
});
