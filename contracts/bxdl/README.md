# BXDL 데이터 계약

BXDL이 소유한 개발용 JSON schema다. NIGO의 엔진 계약과 별개이며 채널은 `development`, 엔진 계약 상태는 `proposed`다. schema는 구조를 표현하고 실제 CLI는 중복 key·경로·권한·정확한 inventory/hash·한도 등 추가 조건을 검사한다.

| Schema | 용도 |
| --- | --- |
| [cli-result](./cli-result.schema.json) | CLI 공통 JSON 결과 envelope |
| [instance](./instance.schema.json) | BXDL 제품 설정. NIGO native 설정과 별도 |
| [network](./network.schema.json) | 공통 체인과 validator/peer 공개 입력 |
| [setup-draft](./setup-draft.schema.json) | 중간 입력을 저장하는 내부 설정 초안 |
| [package-spec](./package-spec.schema.json) | 제작자가 지정한 expected identity. inventory는 builder가 생성 |
| [package-manifest](./package-manifest.schema.json) | 서명 대상 package inventory·제품/엔진/runtime 설명 |
| [runtime-lock](./runtime-lock.schema.json) | 대상 OS/CPU·Java identity·전체 JRE inventory와 공급 정보 |
| [engine-lock](./engine-lock.schema.json) | 신뢰한 JAR/Java hash와 전체 expected engine-info |

package platform은 Mac `darwin/arm64`와 Linux `linux/amd64/glibc`를 구분한다. 형식 허용은 해당 OS의 엔진·서비스 인수와 다르며 현재 범위는 [지원 환경](../../docs/support-matrix.md)을 따른다. 구체적인 준비와 검증은 [패키지 형식](../../docs/package-format.md), [JRE 준비](../../docs/runtime.md), [엔진 검사](../../docs/engine.md)를 참고한다.

setup 초안의 입력이 완성되어도 설치·초기화·엔진 실행 완료를 뜻하지 않는다. 완성 제품 설정은 `instance` schema를 사용하며 `setup` 명령으로 수정·재개한다. 초안·등록 기록을 직접 편집해 상태를 변경하지 않는다.

Mac의 binding/journal과 Linux의 root 제어 기록·공개 `management.json`은 실행기가 소유한 내부 프로토콜이다. 임의 생성·변환·복원하는 interchange 계약이 아니며 Linux descriptor가 공개되어도 private 기록의 쓰기 권한을 주지 않는다. 운영 절차는 [Mac 인스턴스](../../docs/instance.md)와 [Linux 설치·운용](../../docs/linux-operations.md)을 따른다.

`contracts/nigo/`와 `contracts/runtime/`의 날짜·버전별 파일은 수신 당시 원문 snapshot이다. 최신 선정 조합과 실제 검증 범위는 [구현 상태](../../docs/implementation-status.md)에 연결한다. 수신 원문을 현행 제품 안내에 맞추어 소급 수정하지 않는다.
