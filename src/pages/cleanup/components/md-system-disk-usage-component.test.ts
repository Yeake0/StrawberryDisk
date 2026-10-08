// @vitest-environment happy-dom

import { mount } from '@vue/test-utils';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '@/i18n';
import type { DiskInfo } from '@/lib/models/disk';
import { OperatingSystemService } from '@/lib/services/operating-system-service';

import MdSystemDiskUsage from './md-system-disk-usage.vue';

const passthroughStub = { template: '<div><slot /></div>' };

const disk: DiskInfo = {
  name: 'Macintosh HD',
  mountPoint: '/',
  totalBytes: 994_660_000_000,
  availableBytes: 31_610_000_000,
  usedBytes: 963_050_000_000,
};

describe('system disk usage tooltip', () => {
  afterEach(() => vi.restoreAllMocks());

  it.each(['macos', 'linux'] as const)('uses the shared decimal capacity tooltip on %s', platform => {
    vi.spyOn(OperatingSystemService, 'currentPlatform').mockReturnValue(platform);
    const wrapper = mount(MdSystemDiskUsage, {
      props: { disk },
      global: {
        plugins: [i18n],
        stubs: {
          MdIcon: true,
          Tooltip: passthroughStub,
          TooltipContent: passthroughStub,
          TooltipTrigger: passthroughStub,
        },
      },
    });

    const trigger = wrapper.get('.system-disk-usage');
    const rows = wrapper.findAll('.system-disk-tooltip-row');

    expect(trigger.attributes('title')).toBeUndefined();
    expect(trigger.attributes('aria-label')).toBeTruthy();
    expect(wrapper.get('.system-disk-tooltip').text()).toContain('Macintosh HD');
    expect(rows).toHaveLength(3);
    expect(rows.map(row => row.find('strong').text())).toEqual(['963.05 GB', '31.61 GB', '994.66 GB']);
    const tooltip = wrapper.get('.system-disk-tooltip').text();
    const hint = i18n.global.t('systemStatus.diskCapacityHint');
    if (platform === 'macos') expect(tooltip).toContain(hint);
    else expect(tooltip).not.toContain(hint);
    wrapper.unmount();
  });
});
