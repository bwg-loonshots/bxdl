# BXDL 개발 계약 v1

- `cli-result.schema.json`: CLI 공통 JSON envelope.
- `package-spec.schema.json`: builder가 받는 expected identity metadata. inventory는 builder가 생성한다.
- `package-manifest.schema.json`: 서명되는 package inventory·제품·엔진·runtime 설명.
- `instance.schema.json`: 제품 JSON 설정. NIGO canonical schema가 아니다.

모두 development 단계의 BXDL 소비/표현 계약이다. JSON Schema는 구조를 표현하고 실행 코드는 경로·중복 key·정확한 inventory·hash·합계·권한 등 추가 조건을 검증한다. schema 파일만 검사한 결과를 runtime verifier 결과로 대신하지 않는다.

2026-09-17 Rust 전환에서 기존 Linux v1 구조와 CLI envelope를 유지하고 development `darwin/arm64` profile을 추가했다. platform의 oneOf는 Mac none/none과 Linux glibc/2.34 조합을 분리한다. version data의 Rust 구현·Mac 우선 식별은 추가 정보이며 실제 service 인수 결과는 NOT_CHECKED다.

현재 NIGO 공급 계약 fixture는 받지 않았다. `contracts/nigo`를 이미 확정된 명세처럼 채우지 않는다. engine contractStatus는 `proposed`만 허용하고 향후 공급자 manifest와 mapping을 검증한 뒤 공식 소비 형식으로 확장한다.
