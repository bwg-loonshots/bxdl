# 등록 자료 보관과 시작 진단 — 2026-10-22

## 문제와 범위

[RC.2 설치 UX](../results/2026-10-21-macos-rc2-setup.md)의 Run A는 시작 gate에서 파일 열기 대기와 UNKNOWN을 남겼다. 특정 파일과 TCC의 관련성은 미확정이다. 코드상 원본 archive·외부 공개키·engine lock을 등록 이후의 worker와 setup 재개가 다시 읽는 의존은 확인했다. 이번 변경은 그 의존을 새 등록에서 제거하고 마지막 시작 검사 단계를 기록한다. 기존 결과를 PASS로 바꾸지 않는다.

## 새 등록 V2

새 인스턴스의 `trust/package.tar.gz`, `trust/engine-lock.json`, 선택 `trust/public-key.pem`에 검증한 bytes를 보관한다. 설치 패키지 밖의 기존 신뢰 입력으로 먼저 서명·전체 archive·cold·설치 inventory를 검증하고, 새 control root 소유권을 기록한 뒤 보관한다. 디렉터리0700·파일0600, create-new·bounded streaming·hash·fsync를 적용한다. 보관본도 동일 artifact verifier와 설치 inventory 검사로 다시 대조한 뒤 binding/journal을 게시한다.

Binding V2는 원래 경로·hash를 출처로 유지하고 실제 실행 pin을 보관본에 결속한다. 원래 출처 경로는 실행 시 stat/open하지 않는다. 서명·정책·공급 JAR/JRE/계약 identity 검증을 생략하거나 설치 receipt를 신뢰 루트로 삼지 않는다. 실패·부분 복사·불완전 등록은 보존하고 원본으로 자동 fallback하거나 덮어쓰지 않는다. V1은 기존 동작을 유지하고 자동 변환하지 않는다.

archive 한 벌만큼 추가 디스크가 필요하며 현재 한도는1GiB다. 원본은 등록 완료까지 필요하다. native·제품 설정·chain·키/비밀번호 참조·설치 폴더·control·data는 여전히 운용 입력이므로 옮기거나 삭제하면 안 된다. `trust` 복사만으로 다른 경로의 macOS 접근 권한을 보장하지 않는다. 같은 UID의 악의적 변조에 대한 격리를 주장하지 않는다.

## 설치 도우미 재개

등록 전에는 기존 원본 검증을 계속한다. 소유한 등록 root가 생긴 뒤에는 완료된 binding·보관본·현재 운용 입력을 검증해 받은 archive/manifest hash와 원출처 pin 집합을 저장된 plan 전체와 비교한다. 생성 제품 hash, draft, 소유 설치 root와 전체 inventory도 계속 확인한다. source hash/path·신뢰 정책이 다르거나 incomplete root이면 UNKNOWN을 보존한다. 원본을 다시 읽어 문제를 가리는 fallback은 없다.

## 시작 진단

GateConsumed 뒤 작업 진입 전에 INPUT_PINS → PACKAGE_VERIFICATION → COLD_PREFLIGHT → NATIVE_CONFIGURATION → SNAPSHOT_VERIFICATION → FINAL_RECHECK → ENGINE_EXEC를 저장한다. 기존 V1 서비스 journal은 필드 없이 읽는다. status/setup/diagnose는 마지막 진입 단계와 허용된 실패 코드만 표시한다. 단계는 완료 증거나 특정 파일/TCC 원인의 증거가 아니다. 엔진 report·PID·health 검증은 기존대로이며 timeout/UNKNOWN을 자동 성공·재시작으로 바꾸지 않는다.

## 종료 관측

새 인수 helper는 OS의 TCP LISTEN을 조회한다. bind 가능 여부는 별개이며 shutdown 판정을 대신하지 않는다. 빈 결과라도 실행 오류·잘린 출력·도구 부재는 UNKNOWN이다. PID·LaunchAgent·정상 엔진 STOPPED도 따로 확인한다. RC.2 A/B의 원본과 고정 하네스는 변경하지 않는다.

## 검증과 후속

원본 삭제/경로 변경 이후 보관본 검증·setup 재개, signed/unsigned 정책, 보관본 변조·symlink/부분 복사 거절, V1 호환, 단계/오류의 비공개 투영·UNKNOWN 회귀를 검사한다. 실제 시험은 새 CLI 패키지와 기존 RC.2 엔진으로 분리해 원자료 부재 상태의 설치 재개·init/start/stop을 확인한다. 제품 source와 패키지가 바뀌므로 과거 archive 인수를 재사용하지 않는다.

[RC.3 수신](../contracts/nigo/engine-rc3-2026-10-22/README.md)은 별도 고정 자료로 보존한다. 이번 수신은 명시 lock 선택이나 RC.3 package/A7 인수가 아니다. 보호 폴더 접근 원인 규명·기존 등록 이동/upgrade·로그아웃/sleep·안전 제거·전체 G1-M은 후속이다.

구현 후 [새 Mac 시험 결과](../results/2026-10-22-macos-managed-supply.md)에서 원본 공급 경로 부재 상태의 13사례를 통과했다. Rust363개·Python34개·Mac release·Linux 타입 검사가 PASS이며 과거 경로 원인과 RC.3 제품 인수는 별도다.
