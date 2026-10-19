# RC1 수신과 Mac 패키지 인수

2026-10-19 · 기반 BXDL `70f527b` · NIGO RC1 source `540a89f0`.

## 범위

REQ-0003 회신을 수용하고 원격 공급 RC1의 bytes를 BXDL에 명시적으로 고정한다.
기존 후보 snapshot/lock을 보존하고 새 계약 snapshot·engine lock·package spec 예시를 추가한다.
엔진은 다시 빌드하지 않는다. 프로세스 제어·상태 판정은 현재 실행 계약을 엄격히 대조한다.

설치 UX는 pin된 chain의 `registration-mode`와 issuer 목록을 읽어 OPEN/ALLOWLISTED의 빈 목록 의미를 구분한다.
누락·미인식·목록 오류·중복은 공개 등록 자격으로 표시하지 않는다. 공급 엔진의 cold 검사와 초기화가 실제 유효성을 판정한다.
새 필드를 Prepared/Plan/journal에 저장하지 않아 이전 설치 계획의 bytes/hash와 재개 호환성을 유지한다.
입력 변경 시 안내를 확인 불가로 표시하며 기존 apply의 pin 검증은 계속 실패한다. 원문 주소·오류·secret 참조는 안내에 추가하지 않는다.

## 검증

- 원격 asset/태그·문서·source·JAR/manifest/console·계약/inventory를 대조하고 별도 소비자 수신 근거를 남긴다.
- RC1 실제 engine-info 및 15개 health fixture로 기존 parser의 identity·정상 종료·UNKNOWN·큰 decimal 처리를 회귀 검증한다.
- make check, Mac release 빌드, Linux 타입 검사를 수행한다. Linux 실행 지원으로 확대하지 않는다.
- 선정 시험 JRE와 동일 CLI·RC1 JAR을 외부 Ed25519 시험 키로 서명한 package로 조립한다.
- 실제 TTY 전체 14문답, 취소/재개, 등록 정책 안내, 설치/cold/init/start/stop/동일 DB 재시작을 새 test-owned 경로에서 확인한다.
- 실제 JAR cold 검사로 ALLOWLISTED+empty, OPEN+empty, 모드 누락, OPEN+nonempty, 중복 issuer를 구분한다.

## 경계

시험용 Oracle jlink JRE를 고객 재배포 대상으로 선정하지 않는다. 기존 시험 TLS는 만료되어 새 private 시험 PKI를 만든다.
기존 NGVK fixture는 시험 자료로만 재사용하며 원본 data/chain/PKI를 수정하지 않는다.
단일 validator의 consensus auto-start/sync OFF fixture로 lifecycle을 확인하므로 전역 합의는 NOT_CHECKED다.
source-free provisioning/거래 fixture 구현 및 4-validator 제품 인수, 실제 중단/실패 주입·로그아웃·sleep/wake,
전체 G1-M과 정식 공급망·배포 runtime 선정은 다음 작업이다.

근거: [수신 snapshot](../contracts/nigo/engine-rc1-2026-10-19/README.md), [실행 결과](../results/2026-10-19-engine-rc1-adoption.md).
