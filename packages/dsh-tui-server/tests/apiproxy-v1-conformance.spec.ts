import { createApiProxyV1Fixture } from './conformance/apiproxy-v1.fixture.ts'
import { registerAdapterConformance } from './conformance/suite.ts'

registerAdapterConformance('apiproxy-v1', createApiProxyV1Fixture)
