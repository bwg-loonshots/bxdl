# 문서 안내

사용 가이드는 현재 CLI 구현을 기준으로 한다. 지원 여부·완료 판정은 [지원 환경](./support-matrix.md)과 [구현 상태](./implementation-status.md)를 함께 확인한다. 이름이 같은 명령도 Linux와 macOS의 설치 입력·권한·서비스 방식이 다를 수 있다.

## 설치·운용

| 목적 | 문서 |
| --- | --- |
| Linux 공개 패키지 다운로드·서명 검증·설치 | [GitHub Releases에서 curl로 설치](./linux-release.md) |
| Linux 노드 설치·초기화·systemd 운용 | [Linux 설치·운용](./linux-operations.md) |
| 관리 CLI 설치·업데이트·복귀 | [독립 관리 CLI](./management-cli.md) |
| 순차 설치 준비·중단/재개·수정 | [설치 준비](./preparation.md), Linux는 [플랫폼 가이드](./linux-operations.md) |
| 노드 목록·이름과 저장 위치 | [노드 이름](./nodes.md) |
| 명령·JSON·상태·오류·진단 | [CLI 사용법](./cli.md) |
| macOS 패키지 파일 설치 | [Mac 설치](./install.md) |
| macOS 등록·초기화·LaunchAgent | [인스턴스 운용](./instance.md) |
| 제품 설정 초안·기존 Mac 설치 도우미 | [setup 세부 사용법](./setup.md) |
| 공통 체인·validator/peer 자료로 설정 준비 | [네트워크 설정](./network.md) |
| zsh 명령·옵션·이름 완성 | [자동완성](./completion.md) |

비공개 Linux 시험 패키지는 동봉 `START-HERE.md`에서 시작한다. 안내서·결과/개선 양식·개별 시험 기록과 전달 archive는 이 저장소에서 배포하거나 수집하지 않는다.

## 제작·검사·개발

| 목적 | 문서 |
| --- | --- |
| Linux source kit·후보 제작·수신·설치 입력 연결 | [Linux 네이티브 빌드](./linux-native-build.md) |
| 선정 Linux JRE·engine-info cold identity 검사 | [Linux 런타임 검증](./linux-validation.md) |
| Linux 관리자용 저수준 서비스 인터페이스 | [systemd 개발 backend](./linux-service-development.md) |
| 대상 OS별 JRE 고정·staging | [JRE 준비](./runtime.md) |
| package manifest·서명·경로·한도 | [패키지 형식](./package-format.md) |
| 제품/native 설정 결합·engine lock·cold 검사 | [엔진 검사](./engine.md) |
| 제품 JSON·공개 결과 schema | [BXDL 계약](../contracts/bxdl/README.md) |
| 실제 파일이 없는 설정 예제 | [config 예제](../config/examples/README.md) |
| 자동 회귀와 명시 실제 인수 도구 | [인수 도구](../tests/acceptance/README.md) |
| 현재 설계·세부 작업 계획 | [설계 안내](../design/README.md) |

## 과거 자료의 읽는 법

- [구현·검증 변경 이력](./implementation-history.md)은 정리 전 상태 기록이다. 현재 사용법이나 미완료 목록으로 사용하지 않는다.
- 날짜가 붙은 `design/` 문서는 해당 결정과 구현의 배경이다. 현재 우선순위와 적용 범위는 설계 안내에서 확인한다.
- `results/`는 특정 바이트·환경에서 관측한 실행 기록이다. 과거 NOT_RUN/INCOMPLETE를 새 결과로 덮지 않는다.
- `contracts/nigo/`, `contracts/runtime/`는 수신 당시 공급물·계약 snapshot이다. 과거의 후보·검증 범위를 최신 조합에 자동 적용하지 않는다. 공급 원문의 상대 링크는 NIGO 저장소 구조를 기준으로 하므로 BXDL checkout에서 열리지 않을 수 있다.
- [console](./console.md)과 [화면 시안](../design/2026-10-22-integrated-operations-ux-screens.md)은 보존된 제한 기능·설계다. 현재 기본 진입점은 직접 CLI다.
