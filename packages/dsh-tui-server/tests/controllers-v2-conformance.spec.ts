import { createControllersV2Fixture } from './conformance/controllers-v2.fixture.ts'
import { registerAdapterConformance } from './conformance/suite.ts'

registerAdapterConformance('controllers-v2', createControllersV2Fixture)
