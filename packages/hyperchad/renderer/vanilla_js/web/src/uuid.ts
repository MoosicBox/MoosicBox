import { v } from './core';

v.genUuid = crypto.randomUUID.bind(crypto);
