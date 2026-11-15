# 선정 JRE 준비

첫 개발 후보는 **Eclipse Temurin21.0.12.1+1-LTS, macOS arm64 전체 JRE**다.
[선정 이유·책임 분담](../design/2026-10-19-jre-selection.md), [공급 snapshot](../contracts/runtime/temurin-21.0.12.1-macos-arm64/README.md)을 따른다.
사용자 설치 흐름에는 JRE 다운로드/선택 단계를 추가하지 않는다. 아래는 패키지 제작자용이다.

## 1. 공급 입력 준비

신뢰한 저장소의 `packaging/runtime-lock.temurin-21-macos-arm64.json`을 선택한다.
`archive.url`에서 받은 원본의 크기와 SHA256을 **먼저** lock의 값과 대조한다. 공식 체크섬/API의 값도 대조한다.
lock을 검증하지 않은 다운로드 파일에서 자동 생성하면 출처를 인증하지 못한다. 버전 URL만 믿고 latest를 추적하지 않는다.

검증한 공급 archive를 별도 새 private 폴더에 안전하게 추출한다. 선택한 archive는 directory/regular entry만 있으며
전체328 entry,262파일이다. 이 중 Home259파일을 준비하며 외곽3파일의 제외 내역은 snapshot의 `observed-runtime.json`에 보존한다. 대상 Home은 `jdk-21.0.12.1+1-jre/Contents/Home`이다.
이 명령은 tar 추출기가 아니므로 외부 입력을 먼저 임의 경로에 풀지 않는다.

## 2. Rust로 준비 폴더 생성

```sh
bxdl runtime stage \
  --source ./upstream/jdk-21.0.12.1+1-jre/Contents/Home \
  --lock ./packaging/runtime-lock.temurin-21-macos-arm64.json \
  --output ./new-stage --json
```

새 `new-stage/runtime/`와 `new-stage/docs/runtime-provenance.json`을 만든다. 전체 파일 목록·내용과 실행 mode를 검증한다.
출력은 기존 경로를 덮어쓰지 않는다. 실패하면 원인을 확인하고 별도 새 경로로 재시도한다. 자동 삭제/resume은 제공하지 않는다.
결과의 archiveVerification/runtimeExecution은 NOT_PERFORMED다. 성공은 입력 lock과 runtime 파일 일치이며
JRE의 공급 서명, 엔진 호환성, source archive 수신 또는 완성 제품 출시 검증을 뜻하지 않는다.

## 3. 패키지 조립

같은 stage에 Rust CLI `bin/bxdl`, 선택한 고정 엔진 `engine/nigo-node.jar`를 준비한다.
이미 운용 인수한 RC.3 개발 후보에는 [package spec](../packaging/package-spec.rc3-temurin.example.json)과
[engine lock](../packaging/engine-lock.rc3-temurin-development.json)을 사용한다.
[RC.3 수신 기록](../contracts/nigo/engine-rc3-2026-10-22/README.md)의 정확한 source/JAR를 대조하고,
수신 이후의 [명시 선택과 Mac 재인수 설계](../design/2026-10-22-rc3-package-reacceptance.md)를 따른다.
기존 RC.1/RC.2 spec/lock은 당시 후보의 재현용으로 보존하며 서로 혼용하지 않는다.
engine lock은 실제 JAR/Java hash를 채운 development lock이다. 별도 제품 revision은 빌드할 CLI에 맞게 설정한다.

- `packaging/runtime-notices.temurin-21.txt` → `licenses/THIRD_PARTY_NOTICES`
- `packaging/runtime-bom.temurin-21.cdx.json` → `licenses/SBOM.json` (현재 runtime-only 범위를 유지)
- runtime snapshot의 공급 metadata/관측/source-supply 문서 → `docs/runtime/`
- 원본 공급 SBOM → `licenses/temurin-supplier-sbom.json`

이 자료는 runtime 부분의 개발용 안내다. 전체 제품 dependency 목록/NOTICE가 완성된 것으로 취급하지 않는다.
원본 runtime/legal 디렉터리는 staging 도구가 함께 보존한다. 고객 출시에 필요한 대응 소스는
source-supply.json의 고정 입력과 함께 별도 배포 kit에 포함하거나 binary와 동등한 접근 경로로 제공한다.

이후 기존 `package build/verify`로 외부 Ed25519 key 서명과 전체 파일 검증을 수행한다.
서명 key는 stage 밖에 둔다. `setup --install`의 패키지·lock 입력과 일반 설치·초기화·기동 흐름은 그대로다.


## RC.4 개발용 런타임 조립

RC.4는 별도의 [engine lock](../packaging/engine-lock.rc4-temurin-development.json)과
[package spec](../packaging/package-spec.rc4-temurin.example.json)을 명시 선택한다.
선정 JRE는 동일하며 [수신 기록](../contracts/nigo/engine-rc4-2026-11-15/README.md)의 JAR를 재빌드하지 않는다.
[RC4-03 결과](../results/2026-11-15-rc4-runtime-assembly.md)는 새 서명 런타임 조립과 engine-info까지 PASS다.
RC.4 외부 관리 CLI profile·설치·서비스·네트워크 인수는 후속이므로 RC.3의 운용 인수 상태와 구분한다.

내부 실행기 A는 clean commit `d38027a474f1e8d4fb7b1d38ea17972ba88d93af`에서
`BXDL_REVISION`을 같은 full commit으로 지정해 `cargo build --offline --locked --release --target aarch64-apple-darwin`으로 빌드했다.
빌드 전후 source 청결도와 실제 `version --json`을 확인한다. `package build` 자체가 이 source 검사를 수행하지는 않는다.
이번 A는 `umask 077` 정책을 포함하며 실제 초기화 보고서 권한은 RC4-05에서 인수한다.

1. 위 `runtime stage` 절차로 선정 lock의 전체 259파일을 새 stage에 준비한다.
2. `bin/bxdl`에는 고정 A, `engine/nigo-node.jar`에는 RC.4 수신 JAR를 복사한다.
3. 위 NOTICE·runtime SBOM·공급 SBOM과 runtime snapshot의 `README.md`, `supplier-metadata.json`, `observed-runtime.json`, `source-supply.json`을 명시적으로 복사한다. 실행기는0755, 일반 문서는0644로 지정한다.
4. RC.4 spec의 조립용 사본에서 `product.revision`을 A의 full commit으로 채운다. `engine.contractRevision`은 source commit이 아닌 계약 fingerprint다.
5. stage의 A·JAR·Java로 RC.4 lock에 대한 `engine inspect --allow-development`를 수행하고 전체 identity를 대조한다. 이는 DB 초기화나 native 실행 검사가 아니다.
6. stage 밖의 개발용 Ed25519 key로 `package build`를 수행하고 외부 공개키로 `package verify`한다. 실제 manifest 기준 전체 파일·크기·mode·hash와 archive/manifest/A identity를 고정한다.

이번 R은 **269 payload / 326,373,420 bytes**다. 결과 JSON에 전체 runtime lock과 나머지 10파일 inventory를 연결했다.
RC.3의 archive hash나 270파일 수를 재사용하지 않는다. 이후 외부 관리 CLI B에 R의 정확한 pin을 등록하며 A를 B로 덮어쓰거나 그 이유로 R을 재조립하지 않는다.
서명 key가 다른 재조립물은 새로운 archive다. 기존 승인 profile의 pin을 새로 계산한 hash로 임의 교체하는 절차가 아니다.

## 검증 경계

full JRE에 필요한 알려진 모듈이 포함되어도 NIGO 전체 호환성 확인을 대신하지 않는다.
RC.1의 기존 lifecycle 기록과 RC.2의 [Mac package 재인수 이력](../results/2026-10-21-macos-rc2-package.md)을 보존한다. 현재 선택한 RC.3의 같은 새 서명 archive에서 [4-validator·두 복구·wrong-pin](../results/2026-10-22-macos-rc3-package.md)과 [원본 공급 경로 부재 setup13사례](../results/2026-10-22-macos-rc3-setup.md)가 PASS다. setup은 제품 설정을 가져온 범위이며 전체14문답·cancel/hangup은 이번에 재실행하지 않았다. offline Storage A7은 NOT_RUN이고 기존 설치 upgrade·전체 G1-M은 후속이다. 각 후보의 JAR를 재빌드하지 않고 소비자 lifecycle·합의/복구와 공급자 engine/crypto/QBFT 검증을 각각 기록한다.
JRE를 교체하면 같은 JAR여도 새 제품 패키지의 영향 범위를 다시 시험한다.
원본 macOS bundle 서명, Apple notarization, 지원 최소 OS, Linux/Docker 지원은 여기서 확정하지 않는다.
