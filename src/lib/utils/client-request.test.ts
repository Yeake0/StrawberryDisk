import { expect, it } from 'vitest';
import { clientRequestHeaders } from './client-request';

it('encodes optional collected metadata without inventing missing fields', () => {
  const metadata = { locale: 'fr-FR', distribution: 'installed' as const };
  expect(clientRequestHeaders(metadata)).toEqual({
    'Accept-Language': 'fr-FR',
    'x-strawberrydisk-locale': 'fr-FR',
    'x-strawberrydisk-distribution': 'installed',
  });
  expect(metadata).toEqual({ locale: 'fr-FR', distribution: 'installed' });
  expect(clientRequestHeaders({ ...metadata, installId: 'fixture', osVersion: '11', timezone: 'UTC' })).toMatchObject({
    'x-strawberrydisk-install-id': 'fixture',
    'x-strawberrydisk-os-version': '11',
    'x-strawberrydisk-timezone': 'UTC',
  });
});
